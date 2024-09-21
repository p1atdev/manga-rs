use anyhow::Result;

use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::Response;
use url::Url;

use crate::auth::EmptyAuth;
use crate::utils;
use crate::viewer::giga::data::Episode;
use crate::viewer::{ViewerClient, ViewerConfig, ViewerConfigBuilder, ViewerWebsite};

/// Preset websites of GigaViewer
pub enum Website {
    ShonenJumpPlus,
    TonarinoYJ,
    HerosWeb,
    ComicBushi,
    ComicBorder,
    ComicDays,
    ComicAction,
    ComicOgyaaa,
    ComicGardo,
    ComicZenon,
    Feelweb,
    Kuragebunch,
    SundayWebry,
    Magcomi,
}

impl ViewerWebsite for Website {
    fn base_url(&self) -> Url {
        let url = match &self {
            Website::ShonenJumpPlus => "https://shonenjumpplus.com",
            Website::TonarinoYJ => "https://tonarinoyj.jp",
            Website::HerosWeb => "https://viewer.heros-web.com",
            Website::ComicBushi => "https://comicbushi-web.com",
            Website::ComicBorder => "https://comicborder.com",
            Website::ComicDays => "https://comic-days.com",
            Website::ComicAction => "https://comic-action.com",
            Website::ComicOgyaaa => "https://comic-ogyaaa.com",
            Website::ComicGardo => "https://comic-gardo.com",
            Website::ComicZenon => "https://comic-zenon.com",
            Website::Feelweb => "https://feelweb.jp",
            Website::Kuragebunch => "https://kuragebunch.com",
            Website::SundayWebry => "https://www.sunday-webry.com",
            Website::Magcomi => "https://magcomi.com",
        };
        Url::parse(url).unwrap()
    }
}

/// viewer config
#[derive(Debug, Clone)]
pub struct Config {
    base_url: Url,
}

impl ViewerConfig for Config {
    fn create_header(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_str(&utils::UserAgent::Bot.value())?,
        );
        Ok(headers)
    }
}

#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    base_url: Url,
    auth: Option<EmptyAuth>,
}

impl ConfigBuilder {
    /// Create a new ConfigBuilder from preset
    pub fn new(website: Website) -> Self {
        Self {
            base_url: website.base_url(),
            auth: None,
        }
    }

    /// Create a new ConfigBuilder from custom url
    pub fn custom(url: String) -> Result<Self> {
        Ok(Self {
            base_url: Url::parse(&url)?,
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
        }
    }
}

/// ChojuGiga viewer client
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
    fn compose_episode_url(&self, episode_id: &str) -> Url {
        self.config
            .base_url
            .join(&format!("/episode/{}.json", episode_id))
            .unwrap()
    }

    /// Get episode
    pub async fn get_episode(&self, episode_id: &str) -> Result<Episode> {
        let url = self.compose_episode_url(episode_id);
        let res = self.get(url).await?;
        let episode: Episode = serde_json::from_slice(&res.bytes().await?)?;
        Ok(episode)
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use futures::StreamExt as _;
    use indicatif::ParallelProgressIterator;
    use rayon::{
        iter::{IntoParallelRefIterator, ParallelIterator},
        slice::ParallelSliceMut,
    };

    use crate::{
        data::{MangaEpisode, MangaPage},
        io::{pdf::PdfWriter, zip::ZipWriter, EpisodeWriter},
        progress::ProgressConfig,
        solver::ImageSolver,
        viewer::giga::solver::Solver,
    };

    use super::*;

    #[tokio::test]
    async fn test_get_episode() {
        let episode_ids = vec![
            "9324103625676410700",
            "10834108156672080500",
            "16457717013869519536",
            "8603475606564031793",
        ];

        for &episode_id in episode_ids.iter() {
            let config = ConfigBuilder::new(Website::ShonenJumpPlus).build();
            let client = Client::new(config);
            let episode = client.get_episode(episode_id).await.unwrap();
            assert_eq!(episode.id(), episode_id);
            assert!(episode.title().is_some());

            let page = episode.pages();

            for p in page {
                let index = p.index().unwrap();
                let url = p.url().unwrap();
                println!("{}: {}", index, url);
            }
        }
    }

    #[tokio::test]
    async fn test_get_and_solve_pages() -> Result<()> {
        let episode_id = "9324103625676410700";

        let progress = ProgressConfig::default();
        let config = ConfigBuilder::new(Website::ShonenJumpPlus).build();
        let client = Arc::new(Client::new(config));
        let episode = client.get_episode(episode_id).await?;

        let pages = episode.pages();

        println!("Downloading {} pages", pages.len());

        let pages = progress
            .build(pages.len())?
            .wrap_stream(futures::stream::iter(pages))
            .map(|page| {
                let client = client.clone();

                tokio::spawn(async move {
                    let url = page.url()?;
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

        let solver = Arc::new(Solver::new());
        let mut images = pages
            .par_iter()
            .progress_with(progress.build(pages.len())?)
            .map(|(bytes, page)| {
                let image = solver.solve_from_bytes(bytes)?;
                let index = page.index()?;
                Result::<_>::Ok((image, index))
            })
            .collect::<Result<Vec<_>>>()?;
        images.par_sort_by_key(|(_, index)| *index);

        println!("Saving {} pages", images.len());

        tokio::fs::create_dir_all("playground/output/giga_solve").await?;
        progress
            .build(images.len())?
            .wrap_stream(futures::stream::iter(images))
            .map(|(image, index)| async move {
                tokio::spawn(async move {
                    tokio::fs::write(
                        format!("playground/output/giga_solve/{}.png", index),
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

    #[tokio::test]
    async fn test_get_and_solve_and_save_as_zip() -> Result<()> {
        let episode_id = "9324103625676410700";

        let progress = ProgressConfig::default();
        let config = ConfigBuilder::new(Website::ShonenJumpPlus).build();
        let client = Arc::new(Client::new(config));
        let episode = client.get_episode(episode_id).await?;

        let pages = episode.pages();

        println!("Downloading {} pages", pages.len());

        let pages = progress
            .build(pages.len())?
            .wrap_stream(futures::stream::iter(pages))
            .map(|page| {
                let client = client.clone();

                tokio::spawn(async move {
                    let url = page.url()?;
                    let res = client.get(url).await?;
                    let bytes = res.bytes().await?;

                    Result::<_>::Ok(bytes)
                })
            })
            .buffer_unordered(4)
            .map(|bytes| bytes?)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        println!("Solving {} pages", pages.len());

        let solver = Arc::new(Solver::new());
        let images = pages
            .par_iter()
            .progress_with(progress.build(pages.len())?)
            .map(|bytes| {
                let image = solver.solve_from_bytes(bytes)?;
                Result::<_>::Ok(image)
            })
            .collect::<Result<Vec<_>>>()?;

        println!("Saving as zip...");

        let writer = ZipWriter::default();
        writer
            .write(images, "playground/output/giga_solve_2.zip")
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_get_and_solve_and_save_as_pdf() -> Result<()> {
        let episode_id = "9324103625676410700";

        let progress = ProgressConfig::default();
        let config = ConfigBuilder::new(Website::ShonenJumpPlus).build();
        let client = Arc::new(Client::new(config));
        let episode = client.get_episode(episode_id).await?;

        let pages = episode.pages();

        println!("Downloading {} pages", pages.len());

        let pages = progress
            .build(pages.len())?
            .wrap_stream(futures::stream::iter(pages))
            .map(|page| {
                let client = client.clone();

                tokio::spawn(async move {
                    let url = page.url()?;
                    let res = client.get(url).await?;
                    let bytes = res.bytes().await?;

                    Result::<_>::Ok(bytes)
                })
            })
            .buffer_unordered(4)
            .map(|bytes| bytes?)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        println!("Solving {} pages", pages.len());

        let solver = Arc::new(Solver::new());
        let images = pages
            .par_iter()
            .progress_with(progress.build(pages.len())?)
            .map(|bytes| {
                let image = solver.solve_from_bytes(bytes)?;
                Result::<_>::Ok(image)
            })
            .collect::<Result<Vec<_>>>()?;

        println!("Saving as zip...");

        let writer = PdfWriter::default();
        writer
            .write(images, "playground/output/giga_solve_3.pdf")
            .await?;

        Ok(())
    }
}
