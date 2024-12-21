use std::sync::LazyLock;

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use regex::Regex;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::Response;
use scraper::{Html, Selector};
use url::Url;

use crate::auth::EmptyAuth;
use crate::error::ClientError;
use crate::utils;
use crate::viewer::mangaz::data::Episode;
use crate::viewer::{ViewerClient, ViewerConfig, ViewerConfigBuilder, ViewerWebsite};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Website {
    MangaZ,
}

static HOST_TO_WEBSITE: phf::Map<&str, Website> = phf::phf_map! {
    "www.mangaz.com" => Website::MangaZ,
    "vw.mangaz.com" => Website::MangaZ,
};

/// Episode path pattern
/// - https://vw.mangaz.com/virgo/view/<episode-id>/i:<index>
/// - https://vw.mangaz.com/navi/<episode-id>
static EPISODE_PATH_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"/(navi|virgo/view)/(\d+)"#).unwrap());

impl ViewerWebsite<Website> for Website {
    fn host(&self) -> &str {
        match self {
            Website::MangaZ => "www.mangaz.com",
        }
    }

    fn base_url(&self) -> Url {
        let url = match self {
            Website::MangaZ => "https://www.mangaz.com",
        };
        Url::parse(url).unwrap()
    }

    fn lookup(host: &str) -> Option<Website> {
        HOST_TO_WEBSITE.get(host).map(|w| *w)
    }
}

impl Website {
    pub fn viewer_url(&self) -> Url {
        let url = match self {
            Website::MangaZ => format!("https://vw.mangaz.com"),
        };
        Url::parse(&url).unwrap()
    }
}

/// mangaz viewer config
#[derive(Debug, Clone)]
pub struct Config {
    base_url: Url,
    viewer_url: Url,
}

impl ViewerConfig for Config {
    fn create_header(&self) -> Result<HeaderMap, ClientError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_str(&utils::UserAgent::Bot.value())
                .map_err(|_| ClientError::InvalidHeader)?,
        );
        headers.insert(
            header::REFERER,
            HeaderValue::from_str(&self.base_url.to_string())
                .map_err(|_| ClientError::InvalidHeader)?,
        );
        Ok(headers)
    }
}

#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    base_url: Url,
    viewer_url: Url,
    auth: Option<EmptyAuth>,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            base_url: Website::MangaZ.base_url(),
            viewer_url: Website::MangaZ.viewer_url(),
            auth: None,
        }
    }
}

impl ConfigBuilder {
    pub fn new(website: Website) -> Self {
        Self {
            base_url: website.base_url(),
            viewer_url: website.viewer_url(),
            auth: None,
        }
    }

    pub fn custom(base_url: String, viewer_url: String) -> Result<Self> {
        Ok(Self {
            base_url: Url::parse(&base_url)?,
            viewer_url: Url::parse(&viewer_url)?,
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
            viewer_url: self.viewer_url.clone(),
        }
    }
}

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

    /// Parse episode id from url
    fn parse_episode_id(&self, url: &Url) -> Option<String> {
        let path = url.path();
        let captures = EPISODE_PATH_PATTERN.captures(path)?;
        // 1: prefix, 2: episode id, 3: page index
        captures.get(2).map(|m| m.as_str().to_string())
    }
}

impl Client {
    fn compose_episode_url(&self, episode_id: &str) -> Url {
        let url = self
            .config
            .viewer_url
            .join(&format!("/virgo/view/{}", episode_id))
            .unwrap();
        url
    }

    fn decode_base64(&self, data: &str) -> Result<String> {
        let data = STANDARD.decode(data)?;
        let data = String::from_utf8(data)?;
        Ok(data)
    }

    fn extract_data_from_html(&self, html: &str) -> Result<String> {
        let document = Html::parse_document(html);
        let selector = match Selector::parse("span#doc") {
            Ok(selector) => selector,
            Err(_) => bail!("Failed to parse selector"),
        };
        let doc = match document.select(&selector).next() {
            Some(span) => Ok(span.inner_html()),
            None => Err(anyhow!("Failed to find span#doc")),
        }?;
        Ok(doc.trim().to_string())
    }

    /// Get episode
    pub async fn get_episode(&self, episode_id: &str) -> Result<Episode> {
        let url = self.compose_episode_url(episode_id);
        let res = self.get(url).await?;
        let html = res.text().await?;
        let encoded = self.extract_data_from_html(&html)?;
        let decoded = self.decode_base64(&encoded)?;
        let episode = serde_json::from_str(&decoded)?;
        Ok(episode)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::data::{MangaEpisode, MangaPage};

    #[tokio::test]
    async fn test_get_episode() {
        let episode_ids = vec!["211991", "139231", "65031"];

        for &episode_id in episode_ids.iter() {
            let config = ConfigBuilder::new(Website::MangaZ).build();
            let client = Client::new(config);
            let episode = client.get_episode(episode_id).await.unwrap();
            assert_eq!(episode.id(), episode_id);
            assert!(episode.title().is_some());

            let page = episode.pages();
            let base_url = episode.image_base_url().unwrap();
            let verkey = episode.verkey();

            for p in page {
                let index = p.index().unwrap();
                let url = p.image_url(&base_url, &verkey).unwrap();
                println!("{}: {}", index, url);
                // println!("{:?}", p.scramble);
            }
        }
    }
}
