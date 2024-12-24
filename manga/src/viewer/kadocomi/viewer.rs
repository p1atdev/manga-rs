use anyhow::Result;
use reqwest::{
    header::{self, HeaderMap, HeaderValue},
    Response,
};
use scraper::Html;
use url::Url;

use crate::{
    auth::EmptyAuth,
    error::ClientError,
    utils,
    viewer::{ViewerClient, ViewerConfig, ViewerConfigBuilder, ViewerWebsite},
};

use self::utils::extract_next_data_json;

use super::data::{episode::Episode, next::EpisodeNextData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Website {
    Kadocomi,
}

static HOST_TO_WEBSITE: phf::Map<&str, Website> = phf::phf_map! {
    "comic-walker.com" => Website::Kadocomi,
};

impl ViewerWebsite<Website> for Website {
    fn host(&self) -> &str {
        match &self {
            Website::Kadocomi => "comic-walker.com",
        }
    }

    fn base_url(&self) -> url::Url {
        let url = match &self {
            Website::Kadocomi => "https://comic-walker.com",
        };
        url::Url::parse(url).unwrap()
    }

    fn lookup(host: &str) -> Option<Website> {
        HOST_TO_WEBSITE.get(host).map(|w| *w)
    }
}

/// Kadocomi has 2 image size types: width:1280 and width:768.
/// - width:1280 is the large image size. (for PC)
/// - width:768 is the small image size.
#[derive(Debug, Clone, Copy)]
pub enum ImageSize {
    Large,
    Small,
}

impl ImageSize {
    pub fn query_value(&self) -> &str {
        match self {
            ImageSize::Large => "width:1284",
            ImageSize::Small => "width:768",
        }
    }
}

/// viewer config
#[derive(Debug, Clone)]
pub struct Config {
    base_url: Url,
    image_size: ImageSize,
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
    image_size: ImageSize,
}

impl ConfigBuilder {
    /// Create a new ConfigBuilder from preset
    pub fn new(website: Website) -> Self {
        Self {
            base_url: website.base_url(),
            auth: None,
            image_size: ImageSize::Large,
        }
    }

    /// Create a new ConfigBuilder from custom url
    pub fn custom(url: String) -> Result<Self> {
        Ok(Self {
            base_url: Url::parse(&url)?,
            auth: None,
            image_size: ImageSize::Large,
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
            image_size: self.image_size,
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
    async fn get_html(&self, url: Url) -> Result<Html, ClientError> {
        let res = self.get(url).await?;
        let html = Html::parse_document(&res.text().await.map_err(|_| ClientError::DecodeError)?);
        Ok(html)
    }

    /// Compose API viewer URL.
    /// Sample: https://comic-walker.com/api/contents/viewer?episodeId=018d6b94-03f2-7717-955c-7c634f8dead6&imageSizeType=width:1284
    fn compose_api_viewer_url(&self, episode_id: &str) -> Result<Url, ClientError> {
        Ok(self
            .config
            .base_url
            .join(&format!(
                "/api/contents/viewer?episodeId={}&imageSizeType={}",
                episode_id,
                self.config.image_size.query_value()
            ))
            .map_err(|_| ClientError::InvalidUrl)?)
    }

    fn parser_next_data(&self, html: &Html) -> Result<EpisodeNextData> {
        let script = extract_next_data_json(html)?;
        let json: EpisodeNextData = serde_json::from_str(&script)?;

        Ok(json)
    }

    pub async fn get_api_viewer(&self, episode_id: &str) -> Result<Episode, ClientError> {
        let url = self.compose_api_viewer_url(episode_id)?;
        let res = self.get(url.clone()).await?;
        if res.status().is_success() {
            let episode: Episode =
                serde_json::from_slice(&res.bytes().await.map_err(|_| ClientError::DecodeError)?)
                    .map_err(|_| ClientError::DecodeError)?;
            return Ok(episode);
        }

        Err(self.map_error_status(res.status()))
    }

    pub async fn get_next_data(&self, url: Url) -> Result<EpisodeNextData, ClientError> {
        let html = self.get_html(url).await?;
        let data = self
            .parser_next_data(&html)
            .map_err(|e| ClientError::ParseError(e.to_string()))?;

        Ok(data)
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[tokio::test]
    async fn test_get_html() -> Result<()> {
        let client = Client::new(ConfigBuilder::new(Website::Kadocomi).build());
        let url = Url::parse("https://comic-walker.com/detail/KC_000735_S")?;
        let html = client.get_html(url).await?;
        println!("{:?}", html);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_api_viewer_url() -> Result<()> {
        let client = Client::new(ConfigBuilder::new(Website::Kadocomi).build());
        let episode_id = "018d6b94-03f2-7717-955c-7c634f8dead6";
        let url = client.compose_api_viewer_url(episode_id)?;
        let res = client.get(url).await?;

        assert!(res.status().is_success());

        Ok(())
    }

    #[tokio::test]
    async fn test_parser_next_data() -> Result<()> {
        let client = Client::new(ConfigBuilder::new(Website::Kadocomi).build());
        let url = Url::parse("https://comic-walker.com/detail/KC_000735_S")?;
        let html = client.get_html(url).await?;

        let data = client.parser_next_data(&html)?;

        println!("{:?}", data);

        Ok(())
    }

    #[tokio::test]
    async fn test_parser_next_data_easy() -> Result<()> {
        let client = Client::new(ConfigBuilder::new(Website::Kadocomi).build());
        let url = Url::parse("https://comic-walker.com/detail/KC_000735_S")?;
        let data = client.get_next_data(url).await?;

        println!("{:?}", data);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_api_viewer() -> Result<()> {
        let client = Client::new(ConfigBuilder::new(Website::Kadocomi).build());
        let episode_id = "018d6b94-03f2-7717-955c-7c634f8dead6";
        let _data = client.get_api_viewer(episode_id).await?;

        Ok(())
    }
}
