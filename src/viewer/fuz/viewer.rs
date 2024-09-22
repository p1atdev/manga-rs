use std::sync::LazyLock;

use anyhow::Result;

use regex::Regex;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::Response;
use url::Url;

use crate::auth::EmptyAuth;
use crate::utils;
use crate::viewer::{ViewerClient, ViewerConfig, ViewerConfigBuilder, ViewerWebsite};

use super::data::{web_manga_viewer, Episode};

/// ComicFuz website family
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Website {
    ComicFuz,
}

static HOST_TO_WEBSITE: phf::Map<&str, Website> = phf::phf_map! {
    "comic-fuz.com" => Website::ComicFuz,
};

/// Episode path pattern
static EPISODE_PATH_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"/manga/viewer/(\d+)$"#).unwrap());

impl ViewerWebsite<Website> for Website {
    fn host(&self) -> &str {
        match &self {
            Website::ComicFuz => "comic-fuz.com",
        }
    }

    fn base_url(&self) -> Url {
        let url = match &self {
            Website::ComicFuz => "https://comic-fuz.com",
        };
        Url::parse(url).unwrap()
    }

    fn lookup(host: &str) -> Option<Website> {
        HOST_TO_WEBSITE.get(host).map(|w| *w)
    }
}

impl Website {
    // gRPC API endpoint url
    pub fn api_url(&self) -> Url {
        let url = match &self {
            Website::ComicFuz => "https://api.comic-fuz.com",
        };
        Url::parse(url).unwrap()
    }

    /// Image CDN URL
    pub fn img_url(&self) -> Url {
        let url = match &self {
            Website::ComicFuz => "https://img.comic-fuz.com",
        };
        Url::parse(url).unwrap()
    }
}

/// viewer config
#[derive(Debug, Clone)]
pub struct Config {
    base_url: Url,
    api_url: Url,
    img_url: Url,
}

impl ViewerConfig for Config {
    fn create_header(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_str(&utils::UserAgent::Bot.value())?,
        );
        headers.insert(
            header::REFERER,
            HeaderValue::from_str(&self.base_url.to_string())?,
        );
        Ok(headers)
    }
}

#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    base_url: Url,
    api_url: Url,
    img_url: Url,
    auth: Option<EmptyAuth>,
}

impl ConfigBuilder {
    /// comic-fuz.com default config
    pub fn default() -> Self {
        Self {
            base_url: Website::ComicFuz.base_url(),
            api_url: Website::ComicFuz.api_url(),
            img_url: Website::ComicFuz.img_url(),
            auth: None,
        }
    }

    /// Create a new ConfigBuilder from preset
    pub fn new(website: Website) -> Self {
        Self {
            base_url: website.base_url(),
            api_url: website.api_url(),
            img_url: website.img_url(),
            auth: None,
        }
    }

    /// Create a new ConfigBuilder from custom url
    pub fn custom(base_url: String, api_url: String, img_url: String) -> Result<Self> {
        Ok(Self {
            base_url: Url::parse(&base_url)?,
            api_url: Url::parse(&api_url)?,
            img_url: Url::parse(&img_url)?,
            auth: None,
        })
    }
}

impl ViewerConfigBuilder<Config, EmptyAuth> for ConfigBuilder {
    fn set_auth(&mut self, auth: EmptyAuth) -> &mut Self {
        self.auth = Some(auth);
        self
    }

    fn build(&self) -> Config {
        Config {
            base_url: self.base_url.clone(),
            api_url: self.api_url.clone(),
            img_url: self.img_url.clone(),
        }
    }
}

/// ComicFuz viewer client
#[derive(Debug, Clone)]
pub struct Client {
    client: reqwest::Client,
    config: Config,
}

impl ViewerClient<Config> for Client {
    fn new(config: Config) -> Self {
        let client = reqwest::Client::new();
        Self { client, config }
    }

    async fn fetch_raw<B: Into<reqwest::Body> + Send>(
        &self,
        url: Url,
        method: reqwest::Method,
        body: Option<B>,
        headers: Option<HeaderMap>,
    ) -> Result<Response> {
        let mut req = self
            .client
            .request(method, url)
            .headers(self.config.create_header()?);
        if let Some(headers) = headers {
            req = req.headers(headers);
        }
        if let Some(body) = body {
            req = req.body(body);
        }
        let res = req.send().await?.error_for_status()?;
        Ok(res)
    }

    /// Parse episode id from url
    /// - https://comic-fuz.com/manga/viewer/36429
    fn parse_episode_id(&self, url: &Url) -> Option<String> {
        let path = url.path();
        let captures = EPISODE_PATH_PATTERN.captures(path)?;
        captures.get(1).map(|m| m.as_str().to_string())
    }
}

impl Client {
    // API /v1/web_manga_viewer
    fn compose_v1_web_manga_viewer(&self) -> Url {
        self.config.api_url.join("/v1/web_manga_viewer").unwrap()
    }

    /// Image url on CDN
    pub fn image_url(&self, path: String) -> Result<Url> {
        Ok(self.config.img_url.join(&path)?)
    }

    /// Fetch with protobuf
    pub async fn fetch_protobuf<T: prost::Message + Default>(
        &self,
        url: Url,
        message: impl prost::Message,
    ) -> Result<T> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/protobuf"),
        );
        let res = self
            .post(url, message.encode_to_vec(), Some(headers))
            .await?;
        let bytes = res.bytes().await?;
        let message = prost::Message::decode(bytes)?;
        Ok(message)
    }

    async fn api_v1_web_manga_viewer(
        &self,
        message: web_manga_viewer::WebMangaViewerRequest,
    ) -> Result<web_manga_viewer::WebMangaViewerResponse> {
        let url = self.compose_v1_web_manga_viewer();
        self.fetch_protobuf(url, message).await
    }

    /// Get episode
    pub async fn get_episode(&self, episode_id: &str) -> Result<Episode> {
        let message = web_manga_viewer::WebMangaViewerRequest::free_chapter_id(episode_id.parse()?);
        let res = self.api_v1_web_manga_viewer(message).await?;
        let episode = Episode::from(res);
        Ok(episode)
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use anyhow::bail;
    use futures::StreamExt;
    use indicatif::ParallelProgressIterator;
    use rayon::{
        iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator},
        slice::ParallelSliceMut,
    };

    use crate::{
        data::{MangaEpisode, MangaPage},
        progress::ProgressConfig,
        solver::ImageSolver,
        viewer::fuz::{data::Page, solver::Solver},
    };

    use super::*;

    #[tokio::test]
    async fn test_fetch_protobuf() -> Result<()> {
        let chapter_ids = vec!["2443", "36429", "45054", "57443"];

        let config = ConfigBuilder::default().build();
        let client = Client::new(config);

        for chapter_id in chapter_ids {
            let res = client.get_episode(chapter_id).await?;
            println!("{:?}", res);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_and_solve() -> Result<()> {
        let chapter_id = "2443";

        let progress = ProgressConfig::default();
        let config = ConfigBuilder::default().build();
        let client = Arc::new(Client::new(config));
        let episode = client.get_episode(chapter_id).await?;

        let pages = episode
            .pages()
            .into_par_iter()
            .filter(|page| page.is_image())
            .collect::<Vec<_>>();

        println!("Downloading {} pages", pages.len());

        let pages = progress
            .build(pages.len())?
            .wrap_stream(futures::stream::iter(pages))
            .map(|page| {
                let client = client.clone();
                tokio::spawn(async move {
                    let url = client.image_url(page.image_path()?)?;
                    let res = client.get(url).await?;
                    let bytes = res.bytes().await?;
                    Result::<_>::Ok((bytes, page))
                })
            })
            .buffer_unordered(4)
            .map(|pair| pair?)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        println!("Solving {} pages", pages.len());

        let images = pages
            .par_iter()
            .progress_with(progress.build(pages.len())?)
            .map(|(bytes, page)| {
                if let Page::Image(img) = page {
                    println!("Solving page {}", page.index()?);
                    println!("page: {:?}", page);
                    let solver = Solver::new(img.encryption_key(), img.encryption_iv());
                    let image = solver.solve(bytes)?;
                    Result::<_>::Ok((image, page.index()?))
                } else {
                    bail!("Page is not an image")
                }
            })
            .collect::<Result<Vec<_>>>()?;

        println!("Saving {} pages", images.len());

        tokio::fs::create_dir_all("playground/output/fuz_solve").await?;
        progress
            .build(images.len())?
            .wrap_stream(futures::stream::iter(images))
            .map(|(image, index)| {
                tokio::spawn(async move {
                    tokio::fs::write(format!("playground/output/fuz_solve/{}.jpg", index), image)
                        .await
                        .unwrap();
                })
            })
            .buffer_unordered(16)
            .collect::<Vec<_>>()
            .await;

        Ok(())
    }
}
