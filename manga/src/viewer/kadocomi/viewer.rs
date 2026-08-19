use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use scraper::Html;
use url::Url;

use crate::{
    auth::EmptyAuth,
    error::ClientError,
    http::HttpClient,
    utils::{extract_next_data_json, UserAgent},
    viewer::{ViewerConfig, ViewerConfigBuilder},
};

use super::data::{episode::Episode, next::EpisodeNextData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSize {
    Large,
    Small,
}

impl ImageSize {
    pub fn query_value(self) -> &'static str {
        match self {
            ImageSize::Large => "width:1284",
            ImageSize::Small => "width:768",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    base_url: Url,
    image_size: ImageSize,
}

impl Config {
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UserAgent::Bot.value()));
        headers
    }

    fn api_viewer_url(&self, episode_id: &str) -> Result<Url, ClientError> {
        let mut url = self
            .base_url
            .join("api/contents/viewer")
            .map_err(|_| ClientError::InvalidUrl)?;
        url.query_pairs_mut()
            .append_pair("episodeId", episode_id)
            .append_pair("imageSizeType", self.image_size.query_value());
        Ok(url)
    }
}

impl ViewerConfig for Config {
    fn create_header(&self) -> Result<HeaderMap, ClientError> {
        Ok(self.headers())
    }
}

#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    base_url: Url,
    image_size: ImageSize,
}

impl ConfigBuilder {
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            image_size: ImageSize::Large,
        }
    }

    pub fn image_size(mut self, image_size: ImageSize) -> Self {
        self.image_size = image_size;
        self
    }
}

impl ViewerConfigBuilder<Config, EmptyAuth> for ConfigBuilder {
    fn set_auth(&mut self, _auth: EmptyAuth) -> &mut Self {
        self
    }

    fn build(&self) -> Config {
        Config {
            base_url: self.base_url.clone(),
            image_size: self.image_size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    http: HttpClient,
    config: Config,
}

impl Client {
    pub fn new(config: Config) -> Self {
        Self {
            http: HttpClient::new(config.headers()),
            config,
        }
    }

    pub async fn get(&self, url: Url) -> Result<reqwest::Response, ClientError> {
        self.http.get(url).await
    }

    async fn get_html(&self, url: Url) -> Result<Html, ClientError> {
        let response = self.get(url).await?;
        let body = response
            .text()
            .await
            .map_err(|_| ClientError::DecodeError)?;
        Ok(Html::parse_document(&body))
    }

    fn parse_next_data(html: &Html) -> Result<EpisodeNextData, ClientError> {
        let json = extract_next_data_json(html)
            .map_err(|error| ClientError::ParseError(error.to_string()))?;
        serde_json::from_str(&json).map_err(|error| ClientError::ParseError(error.to_string()))
    }

    pub async fn get_api_viewer(&self, episode_id: &str) -> Result<Episode, ClientError> {
        let response = self.get(self.config.api_viewer_url(episode_id)?).await?;
        serde_json::from_slice(
            &response
                .bytes()
                .await
                .map_err(|_| ClientError::DecodeError)?,
        )
        .map_err(|error| ClientError::ParseError(error.to_string()))
    }

    pub async fn get_next_data(&self, url: Url) -> Result<EpisodeNextData, ClientError> {
        let html = self.get_html(url).await?;
        Self::parse_next_data(&html)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        ConfigBuilder::new(Url::parse("http://localhost:4000/").unwrap()).build()
    }

    #[test]
    fn composes_api_url_from_custom_origin() {
        let url = config().api_viewer_url("episode id").unwrap();
        assert_eq!(url.path(), "/api/contents/viewer");
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![
                ("episodeId".into(), "episode id".into()),
                ("imageSizeType".into(), "width:1284".into()),
            ]
        );
    }

    #[test]
    fn parses_next_data_without_network() {
        let html = Html::parse_document(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/kadokomi_next.html"
        )));
        let data = Client::parse_next_data(&html).unwrap();
        assert_eq!(data.episode_id().unwrap(), "episode-1");
        assert_eq!(data.episode_title().unwrap(), "Episode 1");
    }
}
