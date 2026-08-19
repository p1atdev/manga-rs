use anyhow::{Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::Response;
use reqwest::header::{self, HeaderMap, HeaderValue};
use scraper::{Html, Selector};
use url::Url;

use crate::auth::EmptyAuth;
use crate::error::ClientError;
use crate::utils;
use crate::viewer::mangaz::data::Episode;
use crate::viewer::{ViewerClient, ViewerConfig, ViewerConfigBuilder};

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
            HeaderValue::from_static(utils::UserAgent::Bot.value()),
        );
        headers.insert(
            header::REFERER,
            HeaderValue::from_str(self.base_url.as_ref())
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

impl ConfigBuilder {
    pub fn new(base_url: Url, viewer_url: Url) -> Self {
        Self {
            base_url,
            viewer_url,
            auth: None,
        }
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
}

impl Client {
    fn compose_episode_url(&self, episode_id: &str) -> Url {
        self.config
            .viewer_url
            .join(&format!("/virgo/view/{}", episode_id))
            .unwrap()
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
    #[ignore = "MangaZ support is deferred and accesses a live website"]
    async fn test_get_episode() {
        let episode_ids = ["211991", "139231", "65031"];

        for &episode_id in episode_ids.iter() {
            let config = ConfigBuilder::new(
                Url::parse("https://www.mangaz.com").unwrap(),
                Url::parse("https://vw.mangaz.com").unwrap(),
            )
            .build();
            let client = Client::new(config);
            let episode = client.get_episode(episode_id).await.unwrap();
            assert_eq!(episode.id(), episode_id);
            assert!(episode.title().is_some());

            let page = episode.pages();
            let base_url = episode.image_base_url().unwrap();
            let verkey = episode.verkey();

            for p in page {
                let index = p.index().unwrap();
                let url = p.image_url(&base_url, verkey).unwrap();
                println!("{}: {}", index, url);
                // println!("{:?}", p.scramble);
            }
        }
    }
}
