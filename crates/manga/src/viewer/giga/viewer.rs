use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use scraper::{Html, Selector};
use url::Url;

use crate::{
    auth::EmptyAuth,
    error::ClientError,
    feed::{FeedContent, FeedParser},
    http::HttpClient,
    utils::UserAgent,
    viewer::{ViewerConfig, ViewerConfigBuilder},
};

use super::data::{Episode, Gtm, GtmEpisode};

#[derive(Debug, Clone)]
pub struct Config {
    base_url: Url,
}

impl Config {
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UserAgent::Bot.value()));
        headers
    }

    fn episode_url(&self, episode_id: &str) -> Result<Url, ClientError> {
        self.base_url
            .join(&format!("episode/{episode_id}"))
            .map_err(|_| ClientError::InvalidUrl)
    }

    fn series_atom_url(&self, series_id: &str) -> Result<Url, ClientError> {
        self.base_url
            .join(&format!("atom/series/{series_id}"))
            .map_err(|_| ClientError::InvalidUrl)
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
}

impl ConfigBuilder {
    pub fn new(base_url: Url) -> Self {
        Self { base_url }
    }
}

impl ViewerConfigBuilder<Config, EmptyAuth> for ConfigBuilder {
    fn set_auth(&mut self, _auth: EmptyAuth) -> &mut Self {
        self
    }

    fn build(&self) -> Config {
        Config {
            base_url: self.base_url.clone(),
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

    async fn get_feed(&self, url: Url) -> Result<FeedContent, ClientError> {
        let response = self.get(url).await?;
        let body = response
            .text()
            .await
            .map_err(|_| ClientError::DecodeError)?;
        FeedParser::new()
            .parse(&body)
            .map_err(|error| ClientError::ParseError(error.to_string()))
    }

    pub async fn get_episode(&self, episode_id: &str) -> Result<Episode, ClientError> {
        self.get_episode_at(self.config.episode_url(episode_id)?)
            .await
    }

    pub async fn get_episode_at(&self, url: Url) -> Result<Episode, ClientError> {
        let html = self.get_html(url).await?;
        Self::parse_episode_html(&html)
    }

    fn parse_episode_html(html: &Html) -> Result<Episode, ClientError> {
        let selector = Selector::parse("script#episode-json[data-value]")
            .expect("the Giga viewer selector is valid");
        let json = html
            .select(&selector)
            .next()
            .and_then(|element| element.attr("data-value"))
            .ok_or_else(|| ClientError::ParseError("episode-json script not found".to_owned()))?;
        serde_json::from_str(json).map_err(|error| ClientError::ParseError(error.to_string()))
    }

    fn parse_gtm_data(html: &Html) -> Result<GtmEpisode> {
        let selector = Selector::parse("html").expect("the html selector is valid");
        let json = html
            .select(&selector)
            .next()
            .and_then(|element| element.attr("data-gtm-data-layer"))
            .context("data-gtm-data-layer attribute not found")?;
        Ok(serde_json::from_str::<Gtm>(json)?.episode().clone())
    }

    pub async fn get_series_id(&self, url: Url) -> Result<String> {
        let html = self.get_html(url).await?;
        Ok(Self::parse_gtm_data(&html)?.series_id().to_owned())
    }

    pub async fn get_series_feed(&self, series_id: &str) -> Result<FeedContent, ClientError> {
        self.get_feed(self.config.series_atom_url(series_id)?).await
    }
}

#[cfg(test)]
mod tests {
    use crate::data::{MangaEpisode, MangaPage};

    use super::*;

    fn config() -> Config {
        ConfigBuilder::new(Url::parse("http://localhost:4000/").unwrap()).build()
    }

    #[test]
    fn composes_urls_from_custom_origin() {
        assert_eq!(
            config().episode_url("42").unwrap().as_str(),
            "http://localhost:4000/episode/42"
        );
    }

    #[test]
    fn parses_episode_fixture_and_indexes_only_image_pages() {
        let html = Html::parse_document(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/giga_episode.html"
        )));
        let episode = Client::parse_episode_html(&html).unwrap();
        let pages = episode.pages();

        assert_eq!(episode.id(), "episode-1");
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].index().unwrap(), 0);
        assert_eq!(pages[1].index().unwrap(), 1);
    }
}
