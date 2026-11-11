//! Piccoma guest viewer parsing and image restoration.
mod pipeline;
use anyhow::{Context, Result, ensure};
pub use pipeline::{Episode, Page, Pipeline};
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;

// Seedrandom and tile grouping are adapted from eggplants/getpiccoma
// (MIT); see ../../licenses/getpiccoma-MIT.txt.
struct Arc4 {
    state: [u8; 256],
    i: usize,
    j: usize,
}
impl Arc4 {
    fn new(seed: &str) -> Result<Self> {
        ensure!(
            !seed.is_empty() && seed.is_ascii() && seed.len() <= 256,
            "invalid shuffle seed"
        );
        let mut state = [0u8; 256];
        for (i, item) in state.iter_mut().enumerate() {
            *item = i as u8;
        }
        let key = seed.as_bytes();
        let mut j = 0usize;
        for i in 0..256 {
            j = (j + state[i] as usize + key[i % key.len()] as usize) & 255;
            state.swap(i, j);
        }
        let mut arc = Self { state, i: 0, j: 0 };
        for _ in 0..256 {
            arc.byte();
        }
        Ok(arc)
    }
    fn byte(&mut self) -> u8 {
        self.i = (self.i + 1) & 255;
        self.j = (self.j + self.state[self.i] as usize) & 255;
        self.state.swap(self.i, self.j);
        self.state[(self.state[self.i] as usize + self.state[self.j] as usize) & 255]
    }
    fn random(&mut self) -> f64 {
        let mut n = 0f64;
        for _ in 0..6 {
            n = n * 256.0 + f64::from(self.byte());
        }
        let mut denominator = 281_474_976_710_656f64;
        let mut extra = 0u8;
        while n < 4_503_599_627_370_496f64 {
            n = (n + f64::from(extra)) * 256.0;
            denominator *= 256.0;
            extra = self.byte();
        }
        while n >= 9_007_199_254_740_992f64 {
            n /= 2.0;
            denominator /= 2.0;
            extra >>= 1;
        }
        (n + f64::from(extra)) / denominator
    }
}

pub fn shuffle_order(size: usize, seed: &str) -> Result<Vec<usize>> {
    ensure!(size <= 40_000, "too many image tiles");
    let mut random = Arc4::new(seed)?;
    let mut left: Vec<usize> = (0..size).collect();
    let mut out = Vec::with_capacity(size);
    while !left.is_empty() {
        let index = (random.random() * left.len() as f64).floor() as usize;
        ensure!(index < left.len(), "invalid shuffle index");
        out.push(left.remove(index));
    }
    Ok(out)
}

pub fn parse_seed(url: &Url) -> Result<Option<String>> {
    let mut parts = url.path_segments().context("invalid image path")?.rev();
    let _file = parts
        .next()
        .filter(|s| !s.is_empty())
        .context("missing image file")?;
    let checksum = parts.next().context("missing image checksum")?;
    // The raw (still percent-encoded) segment is used; slashes are never decoded.
    ensure!(
        checksum.len() >= 10
            && checksum.len() <= 128
            && checksum.bytes().all(|b| b.is_ascii_graphic()),
        "invalid image checksum"
    );
    let expiry: Vec<_> = url
        .query_pairs()
        .filter(|(k, _)| k == "expires")
        .map(|(_, v)| v.into_owned())
        .collect();
    ensure!(
        expiry.len() == 1
            && !expiry[0].is_empty()
            && expiry[0].len() <= 16
            && expiry[0].bytes().all(|b| b.is_ascii_digit()),
        "invalid image expiry"
    );
    // Equivalent of Python str.isupper(): at least one cased character and no
    // lowercase; digits and punctuation are uncased. Mixed case fails closed.
    let has_lower = checksum.bytes().any(|b| b.is_ascii_lowercase());
    let has_upper = checksum.bytes().any(|b| b.is_ascii_uppercase());
    if has_lower && !has_upper {
        return Ok(None);
    }
    ensure!(has_upper && !has_lower, "invalid image checksum");
    let mut bytes = checksum.as_bytes().to_vec();
    for d in expiry[0].bytes() {
        bytes.rotate_right((d - b'0') as usize);
    }
    let len = bytes.len();
    for (i, ch) in bytes.iter_mut().enumerate() {
        let flip = if i < 10 {
            ![3, 4, 5, 8].contains(&i)
        } else {
            [13, 14, 16].contains(&i) || i == len - 1 || i == len - 2
        };
        if flip {
            *ch ^= 1;
        }
    }
    Ok(Some(String::from_utf8(bytes)?))
}

pub fn restore_image(image: image::RgbaImage, seed: &str) -> Result<image::RgbaImage> {
    use image::GenericImage;
    let (width, height) = image.dimensions();
    ensure!(
        width > 0
            && height > 0
            && width <= 30_000
            && height <= 30_000
            && u64::from(width) * u64::from(height) <= 40_000_000,
        "invalid image size"
    );
    let cols = width.div_ceil(50);
    let rows = height.div_ceil(50);
    type TileGroup = ((u32, u32), Vec<(u32, u32)>);
    let mut groups: Vec<TileGroup> = Vec::new();
    for y in 0..rows {
        for x in 0..cols {
            let px = x * 50;
            let py = y * 50;
            let shape = (50.min(width - px), 50.min(height - py));
            if let Some((_, items)) = groups.iter_mut().find(|(size, _)| *size == shape) {
                items.push((px, py));
            } else {
                groups.push((shape, vec![(px, py)]));
            }
        }
    }
    let mut out = image.clone();
    for ((w, h), tiles) in groups {
        let order = shuffle_order(tiles.len(), seed)?;
        for (dest, src) in tiles.iter().zip(order) {
            let (sx, sy) = tiles[src];
            let cropped = image::imageops::crop_imm(&image, sx, sy, w, h).to_image();
            out.copy_from(&cropped, dest.0, dest.1)?;
        }
    }
    Ok(out)
}

#[derive(Debug, PartialEq, Eq)]
pub struct FreeEpisode {
    pub id: String,
    pub title: String,
}

/// Return only explicitly free entries. Other and ambiguous states are omitted,
/// not classified as unavailable. Viewer access must be checked separately.
pub fn parse_free_episodes(html: &str, product_id: &str) -> Result<Vec<FreeEpisode>> {
    ensure!(
        !product_id.is_empty() && product_id.bytes().all(|c| c.is_ascii_digit()),
        "invalid product ID"
    );
    let document = Html::parse_document(html);
    let list_selector = Selector::parse("ul#js_episodeList").expect("static selector");
    let item_selector = Selector::parse(":scope > li").expect("static selector");
    let link_selector =
        Selector::parse("a[data-product_id][data-episode_id]").expect("static selector");
    let title_selector = Selector::parse(".PCM-epList_title h2").expect("static selector");
    let status_selector = Selector::parse(".PCM-epList_status [class]").expect("static selector");
    let list = document
        .select(&list_selector)
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing Piccoma episode list"))?;
    let mut seen = HashSet::new();
    let mut episodes = Vec::new();
    for item in list.select(&item_selector) {
        let Some(link) = item.select(&link_selector).next() else {
            continue;
        };
        if link.value().attr("data-product_id") != Some(product_id) {
            continue;
        }
        let Some(id) = link.value().attr("data-episode_id") else {
            continue;
        };
        if id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        ensure!(seen.insert(id.to_owned()), "duplicate Piccoma episode ID");
        let mut markers = Vec::new();
        for status in item.select(&status_selector) {
            for class in status.value().classes() {
                if class.starts_with("PCM-epList_status_") {
                    markers.push((class.to_owned(), status.text().collect::<String>()));
                }
            }
        }
        if markers.len() != 1
            || markers[0].0 != "PCM-epList_status_free"
            || markers[0].1.trim() != "¥0"
        {
            continue;
        }
        let Some(title) = item
            .select(&title_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_owned())
            .filter(|title| !title.is_empty())
        else {
            continue;
        };
        episodes.push(FreeEpisode {
            id: id.to_owned(),
            title,
        });
    }
    Ok(episodes)
}

pub struct ViewerPage {
    pub url: Url,
    pub width: u32,
    pub height: u32,
}

pub struct GuestViewer {
    pub scrambled: bool,
    pub pages: Vec<ViewerPage>,
}

/// Validate that a guest viewer response contains page metadata for exactly the
/// requested work and episode. This does not itself establish free status.
pub fn parse_guest_viewer(html: &str, product_id: &str, episode_id: &str) -> Result<GuestViewer> {
    use anyhow::Context;
    for id in [product_id, episode_id] {
        ensure!(
            !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()),
            "invalid Piccoma ID"
        );
    }
    let document = Html::parse_document(html);
    let script_selector = Selector::parse("script").expect("static selector");
    let scripts: Vec<_> = document
        .select(&script_selector)
        .map(|script| script.text().collect::<String>())
        .filter(|script| script.contains("var _pdata_ = {"))
        .collect();
    ensure!(
        scripts.len() == 1,
        "missing or ambiguous Piccoma viewer data"
    );
    let data = scripts[0]
        .split_once("var _pdata_ = {")
        .context("missing viewer data")?
        .1
        .split_once("var _rcm_")
        .context("incomplete viewer data")?
        .0;
    fn field<'a>(data: &'a str, name: &str) -> Result<&'a str> {
        let marker = format!("'{name}':");
        let mut parts = data.split(&marker);
        parts.next();
        let value = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing viewer field"))?;
        ensure!(parts.next().is_none(), "duplicate viewer field");
        Ok(value
            .split_once(',')
            .ok_or_else(|| anyhow::anyhow!("incomplete viewer field"))?
            .0
            .trim())
    }
    ensure!(
        field(data, "product_id")? == product_id && field(data, "episode_id")? == episode_id,
        "viewer ID mismatch"
    );
    let scrambled = match field(data, "isScrambled")? {
        "true" => true,
        "false" => false,
        _ => anyhow::bail!("unknown viewer rendering"),
    };
    let img = data
        .split_once("'img': [")
        .context("missing viewer pages")?
        .1
        .split_once("],")
        .context("incomplete viewer pages")?
        .0;
    let mut pages = Vec::new();
    for line in img.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let line = line
            .strip_prefix("{'path':'")
            .context("invalid viewer page")?;
        let (raw, values) = line.split_once("',").context("invalid viewer page path")?;
        let raw = if raw.starts_with("//") {
            format!("https:{raw}")
        } else {
            raw.to_owned()
        };
        let url = {
            let url = Url::parse(&raw)?;
            ensure!(
                url.scheme() == "https"
                    && url.host_str() == Some("pcm.kakaocdn.net")
                    && url.port().is_none()
                    && url.username().is_empty()
                    && url.password().is_none(),
                "invalid Piccoma image origin"
            );
            url
        };
        let values = values
            .trim()
            .strip_prefix("'width':")
            .context("missing width")?;
        let (width, rest) = values.split_once(',').context("invalid width")?;
        let height = rest
            .trim()
            .strip_prefix("'height':")
            .context("missing height")?
            .strip_suffix("},")
            .context("invalid height")?;
        let width: u32 = width.trim().parse()?;
        let height: u32 = height.trim().parse()?;
        ensure!(
            (1..=30_000).contains(&width) && (1..=30_000).contains(&height),
            "invalid page dimensions"
        );
        pages.push(ViewerPage { url, width, height });
        ensure!(pages.len() <= 200, "too many viewer pages");
    }
    ensure!(!pages.is_empty(), "empty viewer pages");
    Ok(GuestViewer { scrambled, pages })
}

/// Extract IDs from an official Piccoma episode URL.
pub fn episode_ids(url: &Url) -> Result<(String, String)> {
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("piccoma.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "invalid Piccoma episode URL"
    );
    let parts: Vec<_> = url.path().split('/').collect();
    ensure!(
        parts.len() == 5 && parts[0].is_empty() && parts[1] == "web" && parts[2] == "viewer",
        "invalid Piccoma episode path"
    );
    for id in &parts[3..] {
        ensure!(
            !id.is_empty() && id.len() <= 20 && id.bytes().all(|c| c.is_ascii_digit()),
            "invalid Piccoma ID"
        );
    }
    Ok((parts[3].to_owned(), parts[4].to_owned()))
}

/// Decode a page, verify its dimensions and restore it when scrambled.
pub fn solve_page(bytes: &[u8], page: &ViewerPage, scrambled: bool) -> Result<image::DynamicImage> {
    use std::io::Cursor;
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    ensure!(
        width == page.width
            && height == page.height
            && width > 0
            && height > 0
            && width <= 30_000
            && height <= 30_000
            && u64::from(width) * u64::from(height) <= 40_000_000,
        "invalid Piccoma image dimensions"
    );
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(30_000);
    limits.max_image_height = Some(30_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.to_rgba8();
    let image = if scrambled {
        let seed = parse_seed(&page.url)?.context("scrambled Piccoma image has no seed")?;
        restore_image(image, &seed)?
    } else {
        image
    };
    Ok(image::DynamicImage::ImageRgba8(image))
}

#[cfg(test)]
mod client_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn client_confirms_free_status_before_fetching_viewer() {
        let calls = AtomicUsize::new(0);
        let get = |_: String| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok("<ul id='js_episodeList'></ul>".to_string()) }
        };
        assert!(fetch_guest_episode_with("123", "10", get).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn client_fetches_free_episode_and_parses_viewer() {
        let get = |url: String| async move {
            if url.contains("/product/") {
                Ok("<ul id='js_episodeList'><li><a data-product_id='123' data-episode_id='10'><div class='PCM-epList_title'><h2>Free</h2></div><div class='PCM-epList_status'><p class='PCM-epList_status_free'>¥0</p></div></a></li></ul>".to_string())
            } else {
                Ok("<script>var _pdata_ = {'product_id':123,'episode_id':10,'isScrambled':false,'img': [\n{'path':'https://pcm.kakaocdn.net/path/abc1234567/file.jpg?expires=123','width':2,'height':3},\n],}; var _rcm_ = {};</script>".to_string())
            }
        };
        let (title, viewer) = fetch_guest_episode_with("123", "10", get).await.unwrap();
        assert_eq!(title, "Free");
        assert_eq!(viewer.pages.len(), 1);
    }
}

async fn fetch_guest_episode_with<F, Fut>(
    product: &str,
    episode: &str,
    get: F,
) -> Result<(String, GuestViewer)>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    for id in [product, episode] {
        ensure!(
            !id.is_empty() && id.len() <= 20 && id.bytes().all(|c| c.is_ascii_digit()),
            "invalid Piccoma ID"
        );
    }
    let listing = get(format!(
        "https://piccoma.com/web/product/{product}/episodes?etype=E"
    ))
    .await?;
    let free = parse_free_episodes(&listing, product)?;
    let title = free
        .into_iter()
        .find(|item| item.id == episode)
        .context("episode is not confirmed guest-free")?
        .title;
    let html = get(format!(
        "https://piccoma.com/web/viewer/{product}/{episode}"
    ))
    .await?;
    Ok((title, parse_guest_viewer(&html, product, episode)?))
}

/// Guest-only client. Paid, waiting and otherwise ambiguous episodes are rejected.
#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
}

impl Client {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(15))
                .user_agent(crate::utils::UserAgent::Bot.value())
                .build()?,
        })
    }

    async fn get_bounded(&self, url: Url, host: &str, max: usize) -> Result<Vec<u8>> {
        ensure!(
            url.scheme() == "https"
                && url.host_str() == Some(host)
                && url.port().is_none()
                && url.username().is_empty()
                && url.password().is_none(),
            "invalid Piccoma origin"
        );
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Piccoma request failed"))?;
        ensure!(
            response.status().is_success(),
            "Piccoma request returned non-success status"
        );
        ensure!(
            response.content_length().is_none_or(|n| n <= max as u64),
            "Piccoma response too large"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Piccoma response failed"))?
        {
            ensure!(
                bytes.len().saturating_add(chunk.len()) <= max,
                "Piccoma response too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    /// Fetch an episode only if its current listing explicitly marks it free.
    pub async fn get_episode(&self, url: &Url) -> Result<(String, GuestViewer)> {
        let (product, episode) = episode_ids(url)?;
        fetch_guest_episode_with(&product, &episode, |url| async move {
            let bytes = self
                .get_bounded(Url::parse(&url)?, "piccoma.com", 3_000_000)
                .await?;
            Ok(String::from_utf8(bytes)?)
        })
        .await
    }

    /// Fetch a validated viewer image. The caller must first check guest access.
    pub async fn fetch_page(&self, page: &ViewerPage) -> Result<Vec<u8>> {
        self.get_bounded(page.url.clone(), "pcm.kakaocdn.net", 20_000_000)
            .await
    }
}
