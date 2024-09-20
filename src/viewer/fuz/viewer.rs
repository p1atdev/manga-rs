use anyhow::Result;

use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::Response;
use url::Url;

use crate::auth::EmptyAuth;
use crate::utils;
use crate::viewer::{ViewerClient, ViewerConfig, ViewerConfigBuilder, ViewerWebsite};

use super::data::{web_manga_viewer, Episode};

pub enum Website {
    ComicFuz,
}

impl ViewerWebsite for Website {
    fn base_url(&self) -> Url {
        let url = match &self {
            Website::ComicFuz => "https://comic-fuz.com",
        };
        Url::parse(url).unwrap()
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
}

impl Client {
    // API /v1/web_manga_viewer
    fn compose_v1_web_manga_viewer(&self) -> Url {
        self.config.api_url.join("/v1/web_manga_viewer").unwrap()
    }

    /// Image url on CDN
    fn image_url(&self, path: String) -> Result<Url> {
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
    use indicatif::{ParallelProgressIterator, ProgressIterator};
    use rayon::{
        iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator},
        slice::ParallelSliceMut,
    };

    use crate::{
        data::{MangaEpisode, MangaPage},
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

        let config = ConfigBuilder::default().build();
        let client = Arc::new(Client::new(config));
        let episode = client.get_episode(chapter_id).await?;

        let pages = episode
            .pages()
            .into_par_iter()
            .filter(|page| page.is_image())
            .collect::<Vec<_>>();

        println!("Downloading {} pages", pages.len());

        let pbar = indicatif::ProgressBar::new(pages.len() as u64);
        let pages = pbar
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

        let mut images = pages
            .par_iter()
            .progress_count(pages.len() as u64)
            .map(|(bytes, page)| {
                if let Page::Image(img) = page {
                    println!("Solving page {}", page.index()?);
                    println!("page: {:?}", page);
                    let solver = Solver::new(img.encryption_key(), img.encryption_iv());
                    let image = solver.solve_from_bytes(bytes)?;
                    Result::<_>::Ok((image, page.index()?))
                } else {
                    bail!("Page is not an image")
                }
            })
            .collect::<Result<Vec<_>>>()?;
        images.par_sort_by_key(|(_, index)| *index);

        println!("Saving {} pages", images.len());

        let pbar = indicatif::ProgressBar::new(images.len() as u64);

        tokio::fs::create_dir_all("playground/output/fuz_solve").await?;
        pbar.wrap_stream(futures::stream::iter(images))
            .map(|(image, index)| {
                tokio::spawn(async move {
                    tokio::fs::write(
                        format!("playground/output/fuz_solve/{}.jpg", index),
                        image.as_bytes(),
                    )
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
