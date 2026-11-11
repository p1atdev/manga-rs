//! Experimental COMIC FUZ viewer v2.
use anyhow::{Context, Result, ensure};
use prost::Message;
use url::Url;

fn image_url(path: &str) -> Result<Url> {
    ensure!(
        path.starts_with('/') && !path.starts_with("//") && !path.contains('\\'),
        "invalid FUZ image path"
    );
    let url = Url::parse("https://img.comic-fuz.com")?.join(path)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("img.comic-fuz.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none(),
        "unsafe FUZ image URL"
    );
    Ok(url)
}

#[derive(Clone, PartialEq, Message)]
pub struct ViewerResponse {
    #[prost(message, optional, tag = "2")]
    pub viewer_data: Option<ViewerData>,
    #[prost(uint32, tag = "12")]
    pub chapter_id: u32,
}
#[derive(Clone, PartialEq, Message)]
pub struct ViewerData {
    #[prost(message, repeated, tag = "2")]
    pub pages: Vec<ViewerPage>,
}
#[derive(Clone, PartialEq, Message)]
pub struct ViewerPage {
    #[prost(message, optional, tag = "1")]
    pub image: Option<ViewerImage>,
    #[prost(bytes, optional, tag = "2")]
    pub webview: Option<Vec<u8>>,
    #[prost(bytes, optional, tag = "3")]
    pub last_page: Option<Vec<u8>>,
    #[prost(bytes, optional, tag = "4")]
    pub google_ads: Option<Vec<u8>>,
}
#[derive(Clone, PartialEq, Message)]
pub struct ViewerImage {
    #[prost(string, tag = "1")]
    pub image_url: String,
    #[prost(string, tag = "3")]
    pub iv: String,
    #[prost(string, tag = "4")]
    pub encryption_key: String,
    #[prost(uint32, tag = "5")]
    pub image_width: u32,
    #[prost(uint32, tag = "6")]
    pub image_height: u32,
    #[prost(bool, tag = "7")]
    pub is_extra_page: bool,
}
#[derive(Clone)]
pub struct ImagePage {
    pub index: usize,
    pub url: Url,
    pub iv: String,
    pub key: String,
    pub width: u32,
    pub height: u32,
}
impl crate::data::MangaPage for ImagePage {
    fn index(&self) -> Result<usize> {
        Ok(self.index)
    }
    fn is_image(&self) -> bool {
        true
    }
}
pub fn parse_viewer(bytes: &[u8], id: u32) -> Result<Vec<ImagePage>> {
    let response = ViewerResponse::decode(bytes)?;
    ensure!(response.chapter_id == id, "FUZ viewer chapter mismatch");
    let pages = response.viewer_data.context("FUZ viewer missing")?.pages;
    ensure!(
        !pages.is_empty() && pages.len() <= 250,
        "FUZ viewer page count invalid"
    );
    let mut images = Vec::new();
    for page in pages {
        let count = page.image.is_some() as u8
            + page.webview.is_some() as u8
            + page.last_page.is_some() as u8
            + page.google_ads.is_some() as u8;
        ensure!(count == 1, "unknown FUZ viewer page");
        ensure!(page.webview.is_none(), "unsupported FUZ webview page");
        if let Some(image) = page.image {
            ensure!(
                valid_dimensions(image.image_width, image.image_height),
                "invalid FUZ image dimensions"
            );
            ensure!(
                (image.iv.is_empty() && image.encryption_key.is_empty())
                    || (image.iv.len() == 32
                        && image.encryption_key.len() == 64
                        && image.iv.bytes().all(|x| x.is_ascii_hexdigit())
                        && image.encryption_key.bytes().all(|x| x.is_ascii_hexdigit())),
                "invalid FUZ image key"
            );
            images.push(ImagePage {
                index: images.len(),
                url: image_url(&image.image_url)?,
                iv: image.iv,
                key: image.encryption_key,
                width: image.image_width,
                height: image.image_height,
            });
        }
    }
    ensure!(
        !images.is_empty() && images.len() <= 200,
        "FUZ image count invalid"
    );
    Ok(images)
}
#[derive(Clone, PartialEq, Message)]
struct DeviceInfo {
    #[prost(uint32, tag = "3")]
    device_type: u32,
    #[prost(uint32, tag = "6")]
    image_quality: u32,
}
#[derive(Clone, PartialEq, Message)]
struct ViewerRequest {
    #[prost(message, optional, tag = "1")]
    device_info: Option<DeviceInfo>,
    #[prost(message, optional, tag = "3")]
    consume_point: Option<EmptyPoint>,
    #[prost(uint32, tag = "4")]
    chapter_id: u32,
}
#[derive(Clone, PartialEq, Message)]
struct EmptyPoint {}
pub fn viewer_request(chapter_id: u32) -> Vec<u8> {
    ViewerRequest {
        device_info: Some(DeviceInfo {
            device_type: 2,
            image_quality: 1,
        }),
        consume_point: Some(EmptyPoint {}),
        chapter_id,
    }
    .encode_to_vec()
}

fn valid_dimensions(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= 10000
        && height <= 10000
        && u64::from(width) * u64::from(height) <= 40_000_000
}

/// Return the chapter title only when the work explicitly marks it free.
pub fn parse_free_chapter(html: &str, work: u32, chapter: u32) -> Result<Option<String>> {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    let selector = Selector::parse("script#__NEXT_DATA__").expect("static selector");
    let json = doc
        .select(&selector)
        .next()
        .context("FUZ data missing")?
        .inner_html();
    let data: serde_json::Value = serde_json::from_str(&json)?;
    let props = &data["props"]["pageProps"];
    ensure!(
        work > 0 && chapter > 0 && props["manga"]["mangaId"].as_u64() == Some(work.into()),
        "FUZ work mismatch"
    );
    let groups = props["chapters"]
        .as_array()
        .context("FUZ chapters missing")?;
    let mut found = None;
    for group in groups {
        for entry in group["chapters"]
            .as_array()
            .context("invalid FUZ chapter group")?
        {
            if entry["chapterId"].as_u64() != Some(chapter.into()) {
                continue;
            }
            ensure!(found.is_none(), "duplicate FUZ chapter");
            let title = entry["chapterMainName"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .context("FUZ chapter title missing")?;
            let free = entry
                .get("pointConsumption")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|v| v.is_empty());
            found = Some(if free { Some(title.to_owned()) } else { None });
        }
    }
    found.context("FUZ chapter missing")
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::{cookie::CookieStore, header::HeaderValue};
    #[tokio::test]
    async fn paid_chapter_does_not_request_viewer() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let get = |_: bool| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Ok(br#"<script id="__NEXT_DATA__">{"props":{"pageProps":{"manga":{"mangaId":42},"chapters":[{"chapters":[{"chapterId":1,"chapterMainName":"paid","pointConsumption":{"amount":5}}]}]}}}</script>"#.to_vec())
            }
        };
        assert!(fetch_chapter_with(42, 1, get).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unencrypted_pages_decode_and_dimension_mismatches_fail() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            3,
            image::Rgba([1, 2, 3, 255]),
        ));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let mut page = ImagePage {
            index: 0,
            url: Url::parse("https://img.comic-fuz.com/a").unwrap(),
            key: String::new(),
            iv: String::new(),
            width: 2,
            height: 3,
        };
        assert_eq!(
            decode_page(bytes.get_ref().clone(), &page)
                .unwrap()
                .to_rgba8(),
            image.to_rgba8()
        );
        page.width = 3;
        assert!(decode_page(bytes.into_inner(), &page).is_err());
    }

    #[test]
    fn chapter_sessions_do_not_share_viewer_tokens() {
        let first = FuzGuestSession::new().unwrap();
        let second = FuzGuestSession::new().unwrap();
        let api = Url::parse("https://api.comic-fuz.com/v1/web_manga_viewer_2").unwrap();
        let image = Url::parse("https://img.comic-fuz.com/a?ct=1").unwrap();
        let token = HeaderValue::from_static(
            "fuz_image_token=synthetic; Domain=.comic-fuz.com; Secure; Path=/",
        );
        first.cookies.set_cookies(&mut [&token].into_iter(), &api);
        assert!(first.cookies.cookies(&image).is_some());
        assert!(second.cookies.cookies(&image).is_none());
    }

    #[test]
    fn episode_id_round_trips_and_rejects_other_works_and_invalid_forms() {
        use crate::data::MangaEpisode;
        let episode = Episode {
            work: 42,
            chapter: 7,
            title: "test".into(),
            pages: vec![],
        };
        assert_eq!(parse_episode_id(&episode.id(), 42).unwrap(), 7);
        assert_eq!(parse_episode_id("7", 42).unwrap(), 7);
        for id in ["41/7", "042/7", "42/07", "42/0", "42/7/8", "0", "+7"] {
            assert!(parse_episode_id(id, 42).is_err(), "{id}");
        }
    }

    #[test]
    fn session_uses_only_fresh_token_for_protected_images() {
        let store = FuzCookieStore::default();
        let api = Url::parse("https://api.comic-fuz.com/v1/web_manga_viewer_2").unwrap();
        let image = Url::parse("https://img.comic-fuz.com/a?ct=1").unwrap();
        assert!(store.cookies(&image).is_none());
        let valid = HeaderValue::from_static(
            "fuz_image_token=synthetic; Domain=.comic-fuz.com; Secure; Path=/",
        );
        store.set_cookies(&mut [&valid].into_iter(), &api);
        assert!(store.cookies(&image).is_some());
        assert!(
            store
                .cookies(&Url::parse("https://api.comic-fuz.com/").unwrap())
                .is_none()
        );
        assert!(
            store
                .cookies(&Url::parse("https://img.comic-fuz.com/a").unwrap())
                .is_none()
        );
        assert!(
            store
                .cookies(&Url::parse("https://img.comic-fuz.com.evil.example/a?ct=1").unwrap())
                .is_none()
        );
        let expired = HeaderValue::from_static(
            "fuz_image_token=; Domain=.comic-fuz.com; Secure; Path=/; Max-Age=0",
        );
        store.set_cookies(&mut [&expired].into_iter(), &api);
        assert!(store.cookies(&image).is_none());
    }
}

#[derive(Default)]
struct FuzCookieStore {
    jar: reqwest::cookie::Jar,
}
impl reqwest::cookie::CookieStore for FuzCookieStore {
    fn set_cookies(
        &self,
        headers: &mut dyn Iterator<Item = &reqwest::header::HeaderValue>,
        url: &Url,
    ) {
        if url.scheme() != "https"
            || url.host_str() != Some("api.comic-fuz.com")
            || url.port().is_some()
            || url.path() != "/v1/web_manga_viewer_2"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.query().is_some()
        {
            return;
        }
        let accepted: Vec<_> = headers
            .filter(|header| {
                header
                    .to_str()
                    .ok()
                    .and_then(|value| cookie::Cookie::parse(value).ok())
                    .is_some_and(|cookie| {
                        cookie.name() == "fuz_image_token"
                            && cookie.secure() == Some(true)
                            && cookie.domain().is_some_and(|domain| {
                                domain
                                    .trim_start_matches('.')
                                    .eq_ignore_ascii_case("comic-fuz.com")
                            })
                            && cookie.path() == Some("/")
                    })
            })
            .collect();
        self.jar.set_cookies(&mut accepted.into_iter(), url);
    }
    fn cookies(&self, url: &Url) -> Option<reqwest::header::HeaderValue> {
        if url.scheme() != "https"
            || url.host_str() != Some("img.comic-fuz.com")
            || url.port().is_some()
            || url.username() != ""
            || url.password().is_some()
            || url.fragment().is_some()
            || !url
                .query_pairs()
                .any(|(key, value)| key == "ct" && value == "1")
        {
            return None;
        }
        let header = self.jar.cookies(url)?;
        let valid = header
            .to_str()
            .ok()?
            .split(';')
            .filter_map(|part| part.trim().split_once('='))
            .any(|(name, value)| name == "fuz_image_token" && !value.is_empty());
        valid.then_some(header)
    }
}

async fn fetch_chapter_with<F, Fut>(
    work: u32,
    chapter: u32,
    get: F,
) -> Result<(String, Vec<ImagePage>)>
where
    F: Fn(bool) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>>>,
{
    ensure!(work > 0 && chapter > 0, "invalid FUZ ID");
    let detail = get(false).await?;
    let title = parse_free_chapter(std::str::from_utf8(&detail)?, work, chapter)?
        .context("FUZ chapter is not confirmed guest-free")?;
    Ok((title, parse_viewer(&get(true).await?, chapter)?))
}

struct FuzGuestSession {
    client: reqwest::Client,
    cookies: std::sync::Arc<FuzCookieStore>,
}
impl FuzGuestSession {
    fn new() -> Result<Self> {
        let cookies = std::sync::Arc::new(FuzCookieStore::default());
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(crate::utils::UserAgent::Bot.value())
            .cookie_provider(cookies.clone())
            .build()?;
        Ok(Self { client, cookies })
    }
    async fn bounded(&self, mut response: reqwest::Response, max: usize) -> Result<Vec<u8>> {
        ensure!(
            response.status().is_success(),
            "FUZ request returned non-success status"
        );
        ensure!(
            response.content_length().is_none_or(|n| n <= max as u64),
            "FUZ response too large"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("FUZ response failed"))?
        {
            ensure!(
                bytes.len().saturating_add(chunk.len()) <= max,
                "FUZ response too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
    async fn detail(&self, work: u32) -> Result<Vec<u8>> {
        let url = format!("https://comic-fuz.com/manga/{work}");
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("FUZ detail request failed"))?;
        self.bounded(response, 5_000_000).await
    }
    async fn viewer(&self, chapter: u32) -> Result<Vec<u8>> {
        let response = self
            .client
            .post("https://api.comic-fuz.com/v1/web_manga_viewer_2")
            .header("Content-Type", "application/protobuf")
            .header("Referer", "https://comic-fuz.com/")
            .body(viewer_request(chapter))
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("FUZ viewer request failed"))?;
        self.bounded(response, 3_000_000).await
    }
    async fn image(&self, page: &ImagePage) -> Result<Vec<u8>> {
        use reqwest::cookie::CookieStore;
        let url = &page.url;
        ensure!(
            url.scheme() == "https"
                && url.host_str() == Some("img.comic-fuz.com")
                && url.port().is_none()
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none(),
            "invalid FUZ image origin"
        );
        let requires_cookie = url
            .query_pairs()
            .any(|(key, value)| key == "ct" && value == "1");
        ensure!(
            !requires_cookie || self.cookies.cookies(url).is_some(),
            "FUZ guest image token missing or expired"
        );
        let response = self
            .client
            .get(url.clone())
            .header("Referer", "https://comic-fuz.com/")
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("FUZ image request failed"))?;
        self.bounded(response, 20_000_000).await
    }
}

fn decode_page(bytes: Vec<u8>, page: &ImagePage) -> Result<image::DynamicImage> {
    let bytes = if page.key.is_empty() && page.iv.is_empty() {
        bytes
    } else {
        super::crypto::decrypt_aes_cbc(bytes, &page.key, &page.iv)?
    };
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(10_000);
    limits.max_image_height = Some(10_000);
    limits.max_alloc = Some(160_000_000);
    reader.limits(limits);
    let image = reader.decode()?;
    ensure!(
        valid_dimensions(page.width, page.height)
            && image.width() == page.width
            && image.height() == page.height,
        "FUZ image dimensions mismatch"
    );
    Ok(image)
}

pub struct Page {
    image: ImagePage,
    session: std::sync::Arc<FuzGuestSession>,
}
impl crate::data::MangaPage for Page {
    fn index(&self) -> Result<usize> {
        Ok(self.image.index)
    }
    fn is_image(&self) -> bool {
        true
    }
}
pub struct Episode {
    work: u32,
    chapter: u32,
    title: String,
    pages: Vec<Page>,
}
impl crate::data::MangaEpisode<Page> for Episode {
    fn id(&self) -> String {
        format!("{}/{}", self.work, self.chapter)
    }
    // A stable chapter ID is available; a reliable series ordinal is not.
    fn index(&self) -> crate::data::IndexNumber {
        crate::data::IndexNumber::String(self.id())
    }
    fn title(&self) -> Option<String> {
        Some(self.title.clone())
    }
    fn pages(&self) -> &[Page] {
        &self.pages
    }
    fn into_pages(self) -> Vec<Page> {
        self.pages
    }
}
/// A guest-free FUZ v2 pipeline bound to one work ID.
#[derive(Clone)]
pub struct Pipeline {
    work: u32,
    progress: crate::progress::ProgressConfig,
    writer: crate::pipeline::WriterConfig,
    limits: crate::pipeline::DownloadLimits,
}
impl Pipeline {
    pub fn for_work(work: u32) -> Result<Self> {
        use crate::pipeline::{DownloadLimits, SaveFormat, WriterConfig};
        ensure!(work > 0, "invalid FUZ work ID");
        Ok(Self {
            work,
            progress: crate::progress::ProgressConfig::default(),
            writer: WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png),
            limits: DownloadLimits::new(num_cpus::get(), 8),
        })
    }
    async fn episode(&self, chapter: u32) -> Result<Episode> {
        let session = std::sync::Arc::new(FuzGuestSession::new()?);
        let (title, images) = fetch_chapter_with(self.work, chapter, |viewer| {
            let session = &session;
            async move {
                if viewer {
                    session.viewer(chapter).await
                } else {
                    session.detail(self.work).await
                }
            }
        })
        .await?;
        let pages = images
            .into_iter()
            .map(|image| Page {
                image,
                session: session.clone(),
            })
            .collect();
        Ok(Episode {
            work: self.work,
            chapter,
            title,
            pages,
        })
    }
}
impl crate::pipeline::EpisodePipelineBuilder for Pipeline {
    fn set_progress(mut self, value: crate::progress::ProgressConfig) -> Self {
        self.progress = value;
        self
    }
    fn set_writer_config(mut self, value: crate::pipeline::WriterConfig) -> Self {
        self.writer = value;
        self
    }
    fn set_num_threads(mut self, value: usize) -> Self {
        self.limits.set_num_threads(value);
        self
    }
    fn set_num_connections(mut self, value: usize) -> Self {
        self.limits.set_num_connections(value);
        self
    }
}
impl crate::pipeline::EpisodePipeline<Page, Episode> for Pipeline {
    async fn fetch_episode(&self, id: &str) -> Result<Episode, crate::error::ClientError> {
        let chapter =
            parse_episode_id(id, self.work).map_err(|_| crate::error::ClientError::InvalidUrl)?;
        self.episode(chapter)
            .await
            .map_err(|_| crate::error::ClientError::RequestError)
    }
    async fn fetch_image(
        &self,
        page: &Page,
    ) -> Result<crate::utils::Bytes, crate::error::ClientError> {
        page.session
            .image(&page.image)
            .await
            .map_err(|_| crate::error::ClientError::RequestError)
    }
    async fn solve_image_bytes(
        &self,
        bytes: crate::utils::Bytes,
        page: Option<Page>,
    ) -> Result<crate::utils::Bytes> {
        let image = self.solve_image(bytes, page).await?;
        crate::utils::encode_image(&image, image::ImageFormat::Png)
    }
    async fn solve_image(
        &self,
        bytes: crate::utils::Bytes,
        page: Option<Page>,
    ) -> Result<image::DynamicImage> {
        let page = page.context("page is required to solve a FUZ image")?;
        tokio::task::spawn_blocking(move || decode_page(bytes, &page.image)).await?
    }
    fn file_writer<P: AsRef<std::path::Path>>(&self, path: &P) -> Result<crate::io::FileWriter> {
        crate::io::FileWriter::new(&self.writer, path)
    }
    fn progress(&self) -> &crate::progress::ProgressConfig {
        &self.progress
    }
    fn download_limits(&self) -> &crate::pipeline::DownloadLimits {
        &self.limits
    }
    async fn download<P: AsRef<std::path::Path>>(&self, url: &Url, path: &P) -> Result<()> {
        use crate::data::MangaEpisode;
        let chapter = chapter_id(url)?;
        let episode = self.episode(chapter).await?;
        let title = episode.title.clone();
        self.download_pages(episode.into_pages(), self.file_writer(path)?, &title)
            .await
    }
    async fn download_in<P: AsRef<std::path::Path>>(&self, url: &Url, dir: &P) -> Result<()> {
        use crate::data::MangaEpisode;
        let episode = self.episode(chapter_id(url)?).await?;
        let title = episode.title.clone();
        let path = self.writer.output_path(dir, &title)?;
        self.download_pages(episode.into_pages(), self.file_writer(&path)?, &title)
            .await
    }
}
fn chapter_id(url: &Url) -> Result<u32> {
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("comic-fuz.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "invalid FUZ episode URL"
    );
    let parts: Vec<_> = url
        .path_segments()
        .context("invalid FUZ episode path")?
        .collect();
    ensure!(
        parts.len() == 3 && parts[0] == "manga" && parts[1] == "viewer",
        "invalid FUZ episode path"
    );
    let id: u32 = parts[2].parse()?;
    ensure!(
        id > 0 && parts[2] == id.to_string(),
        "invalid FUZ chapter ID"
    );
    Ok(id)
}

// Accept the episode ID returned by MangaEpisode, as well as a chapter ID.
fn parse_episode_id(id: &str, work: u32) -> Result<u32> {
    let chapter = match id.split_once('/') {
        Some((work_id, chapter)) => {
            ensure!(work_id == work.to_string(), "FUZ work ID mismatch");
            chapter
        }
        None => id,
    };
    let parsed: u32 = chapter.parse().context("invalid FUZ chapter ID")?;
    ensure!(
        parsed > 0 && chapter == parsed.to_string(),
        "invalid FUZ chapter ID"
    );
    Ok(parsed)
}
