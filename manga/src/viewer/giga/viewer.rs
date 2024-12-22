use std::sync::LazyLock;

use anyhow::{anyhow, bail, Result};
use regex::Regex;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{IntoUrl, Response, StatusCode};
use scraper::{Html, Selector};
use url::Url;

use crate::auth::EmptyAuth;
use crate::error::{ClientError, HttpError};
use crate::feed::{FeedContent, FeedParser};
use crate::utils;
use crate::viewer::giga::data::Episode;
use crate::viewer::{ViewerClient, ViewerConfig, ViewerConfigBuilder, ViewerWebsite};

use super::data::{Gtm, GtmEpisode};

/// GigaViewer website family
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Website {
    ShonenJumpPlus,
    TonarinoYJ,
    MagaPocket,
    ComicDays,
    Kuragebunch,
    ComicHeros,
    ComicBorder,
    ComicGardo,
    ComicZenon,
    Magcomi,
    ComicAction,
    ComicTrail,
    ComicGrowl,
    Feelweb,
    SundayWebry,
    ComicOgyaaa,
    ComicEarthstar,
    Ourfeel,
    Custom(String),
}

static HOST_TO_WEBSITE: phf::Map<&str, Website> = phf::phf_map! {
    "shonenjumpplus.com" => Website::ShonenJumpPlus,
    "tonarinoyj.jp" => Website::TonarinoYJ,
    "pocket.shonenmagazine.com" => Website::MagaPocket,
    "comic-days.com" => Website::ComicDays,
    "kuragebunch.com" => Website::Kuragebunch,
    "viewer.heros-web.com" => Website::ComicHeros,
    "comicborder.com" => Website::ComicBorder,
    "comic-gardo.com" => Website::ComicGardo,
    "comic-zenon.com" => Website::ComicZenon,
    "magcomi.com" => Website::Magcomi,
    "comic-action.com" => Website::ComicAction,
    "comic-trail.com" => Website::ComicTrail,
    "comic-growl.com" => Website::ComicGrowl,
    "feelweb.jp" => Website::Feelweb,
    "www.sunday-webry.com" => Website::SundayWebry,
    "comic-ogyaaa.com" => Website::ComicOgyaaa,
    "comic-earthstar.com" => Website::ComicEarthstar,
    "ourfeel.jp" => Website::Ourfeel,
};

/// Episode path pattern
static EPISODE_PATH_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"/episode/(\d+)(?:\.json)?$"#).unwrap());

impl ViewerWebsite<Website> for Website {
    fn host(&self) -> &str {
        match &self {
            Website::ShonenJumpPlus => "shonenjumpplus.com",
            Website::TonarinoYJ => "tonarinoyj.jp",
            Website::MagaPocket => "pocket.shonenmagazine.com",
            Website::ComicDays => "comic-days.com",
            Website::Kuragebunch => "kuragebunch.com",
            Website::ComicHeros => "viewer.heros-web.com",
            Website::ComicBorder => "comicborder.com",
            Website::ComicGardo => "comic-gardo.com",
            Website::ComicZenon => "comic-zenon.com",
            Website::Magcomi => "magcomi.com",
            Website::ComicAction => "comic-action.com",
            Website::ComicTrail => "comic-trail.com",
            Website::ComicGrowl => "comic-growl.com",
            Website::Feelweb => "feelweb.jp",
            Website::SundayWebry => "www.sunday-webry.com",
            Website::ComicOgyaaa => "comic-ogyaaa.com",
            Website::ComicEarthstar => "comic-earthstar.com",
            Website::Ourfeel => "ourfeel.jp",
            Website::Custom(host) => host,
        }
    }

    fn base_url(&self) -> Url {
        Url::parse(&format!("https://{}", self.host())).unwrap()
    }

    fn lookup(host: &str) -> Option<Website> {
        HOST_TO_WEBSITE.get(host).map(|w| w.clone())
    }
}
/// viewer config
#[derive(Debug, Clone)]
pub struct Config {
    base_url: Url,
}

impl ViewerConfig for Config {
    fn create_header(&self) -> Result<HeaderMap, ClientError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_str(&utils::UserAgent::Bot.value())
                .map_err(|_| ClientError::InvalidHeader)?,
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
    ) -> Result<Response, ClientError> {
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
        let res = req.send().await.map_err(|_| ClientError::RequestError)?;
        if res.status().is_success() {
            return Ok(res);
        }
        Err(self.map_error_status(res.status()))
    }
}

impl Client {
    /// Get episode id from the provided url.
    /// - https://example.com/episode/123456
    /// - https://example.com/episode/123456.json
    pub fn parse_episode_id(&self, url: &Url) -> Option<String> {
        let path = url.path();
        let captures = EPISODE_PATH_PATTERN.captures(path)?;
        captures.get(1).map(|m| m.as_str().to_string())
    }

    async fn get_html(&self, url: Url) -> Result<Html, ClientError> {
        let res = self.get(url).await?;
        let html = Html::parse_document(&res.text().await.map_err(|_| ClientError::DecodeError)?);
        Ok(html)
    }

    async fn get_feed(&self, url: Url) -> Result<FeedContent, ClientError> {
        let res = self.get(url).await?;
        let parser = FeedParser::new();
        let feed = parser
            .parse(&res.text().await.map_err(|_| ClientError::DecodeError)?)
            .map_err(|e| ClientError::ParseError(format!("Failed to parse feed: {}", e)))?;

        Ok(feed)
    }

    fn compose_episode_url(&self, episode_id: &str) -> Url {
        self.config
            .base_url
            .join(&format!("/episode/{}", episode_id))
            .unwrap()
    }

    /// Get episode
    pub async fn get_episode(&self, episode_id: &str) -> Result<Episode, ClientError> {
        let url = self.compose_episode_url(episode_id);
        let html = self.get_html(url).await?;
        self.get_episode_from_html(&html)
    }

    fn extract_episode_json(&self, html: &Html) -> Result<String, ClientError> {
        let selector = Selector::parse("script#episode-json").map_err(|_| {
            ClientError::ParseError("Failed to parse selector script#episode-json".to_string())
        })?;
        let json = match html.select(&selector).next() {
            Some(element) => Ok(element.attr("data-value").ok_or(ClientError::ParseError(
                "Failed to extract data-value".to_string(),
            ))?),
            None => Err(ClientError::ParseError(
                "Failed to find script#episode-json".to_string(),
            )),
        }?;
        Ok(json.to_string())
    }

    fn get_episode_from_html(&self, html: &Html) -> Result<Episode, ClientError> {
        let json = self.extract_episode_json(&html)?;
        let episode: Episode = serde_json::from_str(&json)
            .map_err(|_| ClientError::ParseError("Failed to parse episode json".to_string()))?;
        Ok(episode)
    }

    fn compose_series_atom_url(&self, series_id: &str) -> Url {
        // https://shonenjumpplus.com/atom/series/9324103629152359675?free_only=1
        self.config
            .base_url
            .join(&format!("/atom/series/{}", series_id))
            .unwrap()
    }

    fn extract_data_gtm_data_layer(&self, html: &Html) -> Result<String> {
        let selector = match Selector::parse("html") {
            Ok(selector) => selector,
            Err(_) => bail!("Failed to parse selector"),
        };
        let gtm_data_layer = match html.select(&selector).next() {
            Some(element) => Ok(element
                .attr("data-gtm-data-layer")
                .ok_or(anyhow!("Failed to extract data-gtm-data-layer"))?),
            None => Err(anyhow!("Failed to find html")),
        }?;

        Ok(gtm_data_layer.to_string())
    }

    async fn get_gtm_data_from_html(&self, html: &Html) -> Result<GtmEpisode> {
        let gtm_data = self.extract_data_gtm_data_layer(&html)?;
        let gtm_data: Gtm = serde_json::from_str(&gtm_data)?;
        Ok(gtm_data.episode().clone())
    }

    pub async fn get_series_id(&self, url: Url) -> Result<String> {
        let html = self.get_html(url).await?;
        let gtm_data = self.get_gtm_data_from_html(&html).await?;
        Ok(gtm_data.series_id().to_string())
    }

    pub async fn get_series_feed(&self, series_id: &str) -> Result<FeedContent, ClientError> {
        let url = self.compose_series_atom_url(&series_id);
        self.get_feed(url).await
    }
}

#[cfg(test)]
mod test {
    use std::{path::Path, sync::Arc};

    use futures::{StreamExt as _, TryStreamExt};
    use indicatif::ParallelProgressIterator;
    use rayon::{
        iter::{IntoParallelRefIterator, ParallelIterator},
        slice::ParallelSliceMut,
    };

    #[cfg(feature = "pdf")]
    use crate::io::pdf::PdfWriter;
    use crate::{
        data::{MangaEpisode, MangaPage},
        io::{raw::RawWriter, zip::ZipWriter, EpisodeWriter},
        progress::ProgressConfig,
        solver::ImageSolver,
        viewer::giga::solver::Solver,
    };

    use super::*;

    #[tokio::test]
    async fn test_get_html() -> Result<()> {
        let client = Client::new(ConfigBuilder::new(Website::ShonenJumpPlus).build());
        let url = Url::parse("https://shonenjumpplus.com/episode/9324103658562874562")?;
        let html = client.get_html(url).await?;
        println!("{:?}", html);

        Ok(())
    }

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

                async move {
                    let url = page.url()?;
                    let res = client.get(url).await?;
                    let bytes = res.bytes().await?;

                    Result::<_>::Ok((bytes, page))
                }
            })
            .buffer_unordered(4)
            .try_collect::<Vec<_>>()
            .await?;

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
        let images = images
            .into_iter()
            .map(|(image, _)| image)
            .collect::<Vec<_>>();

        println!("Saving {} pages", images.len());

        tokio::fs::create_dir_all("tests/output/giga_solve_raw").await?;
        let writer = Arc::new(RawWriter::default(&Path::new(
            "tests/output/giga_solve_raw",
        )));

        progress
            .build(images.len())?
            .wrap_stream(futures::stream::iter(images))
            .enumerate()
            .map(|(i, image)| {
                let writer = writer.clone();
                async move { writer.write_page(i, image).await }
            })
            .buffer_unordered(num_cpus::get())
            .try_collect::<Vec<_>>()
            .await?;

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

                async move {
                    let url = page.url()?;
                    let res = client.get(url).await?;
                    let bytes = res.bytes().await?;

                    Result::<_>::Ok(bytes)
                }
            })
            .buffer_unordered(4)
            .try_collect::<Vec<_>>()
            .await?;

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

        let writer = Arc::new(ZipWriter::default(&Path::new(
            "tests/output/giga_solve_2.zip",
        ))?);
        progress
            .build(images.len())?
            .wrap_stream(futures::stream::iter(images))
            .enumerate()
            .map(|(i, image)| {
                let writer = writer.clone();
                async move { writer.write_page(i, image).await }
            })
            .buffer_unordered(num_cpus::get())
            .try_collect::<Vec<_>>()
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_get_series_feed() -> Result<()> {
        let episode_id = "9324103625676410700";

        let config = ConfigBuilder::new(Website::ShonenJumpPlus).build();
        let client = Client::new(config);
        let series_id = client
            .get_series_id(client.compose_episode_url(episode_id))
            .await?;
        let feed = client.get_series_feed(&series_id).await?;
        feed.entries().iter().for_each(|entry| {
            assert!(entry.title().is_some());
            entry.links().iter().for_each(|link| {
                assert!(link.url() != "");
            });
        });

        Ok(())
    }

    #[cfg(feature = "pdf")]
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

                async move {
                    let url = page.url()?;
                    let res = client.get(url).await?;
                    let bytes = res.bytes().await?;

                    Result::<_>::Ok(bytes)
                }
            })
            .buffer_unordered(4)
            .try_collect::<Vec<_>>()
            .await?;

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
            .write_images(images, "tests/output/giga_solve_3.pdf")
            .await?;

        Ok(())
    }
}
