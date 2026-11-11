//! Magazine Pocket anonymous guest-free viewer support.
mod pipeline;
use anyhow::{Context, Result, ensure};
pub use pipeline::{Episode, Page, Pipeline};
use url::Url;

fn validate_origin(raw: &str, host: &str) -> Result<Url> {
    let url = Url::parse(raw)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some(host)
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "invalid Magazine Pocket origin"
    );
    Ok(url)
}

/// Reproduce the site's currently observed seed and 4x4 tile permutation.
/// Validated against the WebAssembly viewer's draw calls, not a guessed shuffle.
pub fn tile_permutation(title: u32, episode: u32, seed: &str) -> Result<[usize; 16]> {
    ensure!(
        title > 0 && episode > 0 && !seed.is_empty() && seed.len() <= 10,
        "invalid tile seed"
    );
    let alphabet = if title & 1 == 1 {
        b"q6jtf2xnog"
    } else {
        b"svdk0m7acl"
    };
    let mut decoded = 0u32;
    for ch in seed.bytes() {
        let digit = alphabet
            .iter()
            .position(|c| *c == ch)
            .context("unknown tile seed character")? as u32;
        decoded = decoded
            .checked_mul(10)
            .and_then(|n| n.checked_add(digit))
            .context("tile seed overflow")?;
    }
    let mut state = decoded ^ title.wrapping_add(episode);
    let mut order = [(0u32, 0usize); 16];
    for (idx, item) in order.iter_mut().enumerate() {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *item = (state, idx);
    }
    order.sort_by_key(|p| p.0);
    Ok(order.map(|p| p.1))
}

pub fn restore_image(
    mut image: image::RgbaImage,
    title: u32,
    episode: u32,
    seed: &str,
) -> Result<image::RgbaImage> {
    let permutation = tile_permutation(title, episode, seed)?;
    let (width, height) = image.dimensions();
    ensure!(
        width >= 32 && height >= 32 && u64::from(width) * u64::from(height) <= 100_000_000,
        "invalid image dimensions"
    );
    ensure_opaque(&image)?;
    let tile_width = (width / 32) * 8;
    let tile_height = (height / 32) * 8;
    let source = image.clone();
    for (destination, source_index) in permutation.iter().copied().enumerate() {
        let sx = (source_index as u32 % 4) * tile_width;
        let sy = (source_index as u32 / 4) * tile_height;
        let dx = (destination as u32 % 4) * tile_width;
        let dy = (destination as u32 / 4) * tile_height;
        for y in 0..tile_height {
            for x in 0..tile_width {
                image.put_pixel(dx + x, dy + y, *source.get_pixel(sx + x, sy + y));
            }
        }
    }
    Ok(image)
}

pub fn viewer_request_hash(episode: u32) -> String {
    use sha2::{Digest, Sha256, Sha512};
    fn pair(key: &str, value: &str) -> String {
        format!(
            "{}_{}",
            hex::encode(Sha256::digest(key.as_bytes())),
            hex::encode(Sha512::digest(value.as_bytes()))
        )
    }
    let fields = format!(
        "{},{}",
        pair("episode_id", &episode.to_string()),
        pair("platform", "3")
    );
    let first = hex::encode(Sha256::digest(fields.as_bytes()));
    hex::encode(Sha512::digest(
        format!("{first}{}", pair("", "")).as_bytes(),
    ))
}

#[derive(serde::Deserialize)]
pub struct Viewer {
    status: String,
    response_code: u32,
    title_id: u32,
    episode_id: u32,
    pub scramble_seed: String,
    pub page_list: Vec<String>,
}
pub fn parse_viewer(json: &str, title: u32, episode: u32) -> Result<Viewer> {
    let viewer: Viewer = serde_json::from_str(json).context("invalid viewer metadata")?;
    ensure!(
        viewer.status == "success"
            && viewer.response_code == 0
            && viewer.title_id == title
            && viewer.episode_id == episode,
        "viewer response mismatch"
    );
    ensure!(
        (1..=200).contains(&viewer.page_list.len()),
        "invalid viewer page count"
    );
    tile_permutation(title, episode, &viewer.scramble_seed)?;
    for image in &viewer.page_list {
        validate_origin(image, "mgpk-cdn.magazinepocket.com")?;
    }
    Ok(viewer)
}

pub fn decode_image(bytes: &[u8]) -> Result<image::RgbaImage> {
    use std::io::Cursor;
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    ensure!(
        (32..=8192).contains(&width)
            && (32..=8192).contains(&height)
            && u64::from(width) * u64::from(height) <= 20_000_000,
        "image dimensions outside limits"
    );
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.to_rgba8();
    ensure_opaque(&image)?;
    Ok(image)
}

// The official opaque canvas draws the original before drawing permuted tiles.
// Transparent pixels would therefore composite with earlier destination pixels.
// Reject them until that rendering is independently reproduced.
fn ensure_opaque(image: &image::RgbaImage) -> Result<()> {
    ensure!(
        image.pixels().all(|pixel| pixel.0[3] == 255),
        "transparent Magazine Pocket image unsupported"
    );
    Ok(())
}

/// The site's unauthenticated Web detail endpoint must explicitly report every
/// access field; an absent, malformed or mismatched response is never public.
pub fn parse_episode_detail(json: &str, title: u32, episode: u32) -> Result<bool> {
    #[derive(serde::Deserialize)]
    struct DetailResponse {
        status: String,
        response_code: u32,
        episode: Detail,
    }
    #[derive(serde::Deserialize)]
    struct Detail {
        title_id: u32,
        episode_id: u32,
        point: u32,
        is_page_visible: u8,
        ticket_rental_enabled: u8,
        is_available_by_subscribe: u8,
    }
    let data: DetailResponse = serde_json::from_str(json).context("invalid episode detail")?;
    ensure!(
        data.status == "success"
            && data.response_code == 0
            && data.episode.title_id == title
            && data.episode.episode_id == episode,
        "episode detail mismatch"
    );
    let e = data.episode;
    ensure!(
        e.is_page_visible <= 1 && e.ticket_rental_enabled <= 1 && e.is_available_by_subscribe <= 1,
        "invalid episode detail access flag"
    );
    Ok(e.point == 0
        && e.is_page_visible == 1
        && e.ticket_rental_enabled == 0
        && e.is_available_by_subscribe == 0)
}

/// Parse an official episode URL and validate its canonical numeric IDs.
pub fn episode_ids(url: &Url) -> Result<(u32, u32)> {
    validate_origin(url.as_str(), "pocket.shonenmagazine.com")?;
    ensure!(
        url.query().is_none() && url.fragment().is_none(),
        "invalid Magazine Pocket episode URL"
    );
    let parts: Vec<_> = url
        .path_segments()
        .context("invalid episode path")?
        .collect();
    ensure!(
        parts.len() == 4
            && parts[0] == "title"
            && parts[2] == "episode"
            && parts[1].len() == 5
            && parts[1].bytes().all(|b| b.is_ascii_digit())
            && !parts[3].is_empty()
            && parts[3].bytes().all(|b| b.is_ascii_digit()),
        "invalid episode path"
    );
    let title: u32 = parts[1].parse()?;
    let episode: u32 = parts[3].parse()?;
    ensure!(
        (1..=99_999).contains(&title)
            && (1..=999_999_999).contains(&episode)
            && parts[3] == episode.to_string(),
        "invalid episode ID"
    );
    Ok((title, episode))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[tokio::test]
    async fn guest_client_rejects_paid_episode_before_viewer_request() {
        let calls = AtomicUsize::new(0);
        let get = |_: bool| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Ok(r#"{"status":"success","response_code":0,"episode":{"title_id":1379,"episode_id":320763,"point":90,"is_page_visible":1,"ticket_rental_enabled":0,"is_available_by_subscribe":0}}"#.to_owned())
            }
        };
        assert!(fetch_guest_episode_with(1379, 320763, get).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

async fn fetch_guest_episode_with<F, Fut>(title: u32, episode: u32, get: F) -> Result<Viewer>
where
    F: Fn(bool) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    ensure!(
        (1..=99_999).contains(&title) && (1..=999_999_999).contains(&episode),
        "invalid episode ID"
    );
    let detail = get(false).await?;
    ensure!(
        parse_episode_detail(&detail, title, episode)?,
        "episode is not confirmed guest-free"
    );
    parse_viewer(&get(true).await?, title, episode)
}

/// Guest-only client. Paid, ticketed, subscription and ambiguous episodes are rejected.
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
    async fn get_bounded(
        &self,
        url: Url,
        host: &str,
        max: usize,
        episode: Option<u32>,
    ) -> Result<Vec<u8>> {
        validate_origin(url.as_str(), host)?;
        let mut request = self.client.get(url);
        if let Some(episode) = episode {
            request = request
                .header("x-manga-hash", viewer_request_hash(episode))
                .header("x-manga-platform", "3")
                .header("x-manga-is-crawler", "false");
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Magazine Pocket request failed"))?;
        ensure!(
            response.status().is_success(),
            "Magazine Pocket request returned non-success status"
        );
        ensure!(
            response.content_length().is_none_or(|n| n <= max as u64),
            "Magazine Pocket response too large"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Magazine Pocket response failed"))?
        {
            ensure!(
                bytes.len().saturating_add(chunk.len()) <= max,
                "Magazine Pocket response too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
    pub async fn get_episode(&self, url: &Url) -> Result<Viewer> {
        let (title, episode) = episode_ids(url)?;
        fetch_guest_episode_with(title, episode, |viewer| async move {
            let (host, path, max) = if viewer {
                (
                    "se-api.pocket.shonenmagazine.com",
                    "/web/episode/viewer",
                    3_000_000,
                )
            } else {
                ("api.pocket.shonenmagazine.com", "/web/episode", 100_000)
            };
            let mut url = Url::parse(&format!("https://{host}{path}"))?;
            url.query_pairs_mut()
                .append_pair("platform", "3")
                .append_pair("episode_id", &episode.to_string());
            Ok(String::from_utf8(
                self.get_bounded(url, host, max, Some(episode)).await?,
            )?)
        })
        .await
    }
    /// Fetch a viewer image. The caller must first check guest access.
    pub async fn fetch_page(&self, url: &str) -> Result<Vec<u8>> {
        let url = validate_origin(url, "mgpk-cdn.magazinepocket.com")?;
        self.get_bounded(url, "mgpk-cdn.magazinepocket.com", 20_000_000, None)
            .await
    }
}
