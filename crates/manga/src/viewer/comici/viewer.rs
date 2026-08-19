use reqwest::{
    Body, Method,
    header::{HeaderMap, HeaderValue, REFERER, USER_AGENT},
};
use scraper::{Html, Selector};
use url::Url;

use crate::{
    auth::EmptyAuth,
    error::ClientError,
    http::HttpClient,
    utils::UserAgent,
    viewer::{ViewerConfig, ViewerConfigBuilder},
};

use super::data::{ContentsInfo, Episode, Page};

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
            .join(&format!("episodes/{episode_id}"))
            .map_err(|_| ClientError::InvalidUrl)
    }

    fn contents_info_url(&self, viewer_id: &str, page_to: usize) -> Result<Url, ClientError> {
        let mut url = self
            .base_url
            .join("api/book/contentsInfo")
            .map_err(|_| ClientError::InvalidUrl)?;
        url.query_pairs_mut()
            .append_pair("user-id", "")
            .append_pair("comici-viewer-id", viewer_id)
            .append_pair("page-from", "0")
            .append_pair("page-to", &page_to.to_string());
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct EpisodeMetadata {
    viewer_id: String,
    title: String,
}

impl Client {
    pub fn new(config: Config) -> Self {
        Self {
            http: HttpClient::new(config.headers()),
            config,
        }
    }

    async fn get_html(&self, url: Url) -> Result<Html, ClientError> {
        let response = self.http.get(url).await?;
        let body = response
            .text()
            .await
            .map_err(|_| ClientError::DecodeError)?;
        Ok(Html::parse_document(&body))
    }

    fn parse_episode_html(html: &Html) -> Result<EpisodeMetadata, ClientError> {
        let viewer_selector = Selector::parse("div#comici-viewer[data-comici-viewer-id]")
            .expect("the Comici viewer selector is valid");
        let viewer_id = html
            .select(&viewer_selector)
            .next()
            .and_then(|element| element.attr("data-comici-viewer-id"))
            .filter(|viewer_id| !viewer_id.is_empty())
            .ok_or_else(|| ClientError::ParseError("Comici viewer ID was not found".to_owned()))?
            .to_owned();

        let title_selector = Selector::parse("title").expect("the title selector is valid");
        let title = html
            .select(&title_selector)
            .next()
            .map(|element| element.text().collect::<String>())
            .map(|title| title.trim().to_owned())
            .filter(|title| !title.is_empty())
            .ok_or_else(|| ClientError::ParseError("document title was not found".to_owned()))?;

        Ok(EpisodeMetadata { viewer_id, title })
    }

    async fn get_contents_info(
        &self,
        viewer_id: &str,
        page_to: usize,
    ) -> Result<ContentsInfo, ClientError> {
        let response = self
            .http
            .get(self.config.contents_info_url(viewer_id, page_to)?)
            .await?;
        serde_json::from_slice(
            &response
                .bytes()
                .await
                .map_err(|_| ClientError::DecodeError)?,
        )
        .map_err(|error| ClientError::ParseError(error.to_string()))
    }

    pub async fn get_episode(&self, episode_id: &str) -> Result<Episode, ClientError> {
        self.get_episode_at(self.config.episode_url(episode_id)?)
            .await
    }

    pub async fn get_episode_at(&self, referer: Url) -> Result<Episode, ClientError> {
        let metadata = {
            let html = self.get_html(referer.clone()).await?;
            Self::parse_episode_html(&html)?
        };
        let first = self.get_contents_info(&metadata.viewer_id, 1).await?;
        if first.total_pages == 0 {
            return Err(ClientError::ParseError(
                "Comici episode contains no pages".to_owned(),
            ));
        }

        let contents = self
            .get_contents_info(&metadata.viewer_id, first.total_pages)
            .await?;
        if contents.total_pages != first.total_pages {
            return Err(ClientError::ParseError(
                "Comici total page count changed while loading the episode".to_owned(),
            ));
        }
        if contents.result.len() != contents.total_pages {
            return Err(ClientError::ParseError(format!(
                "Comici returned {} pages, expected {}",
                contents.result.len(),
                contents.total_pages
            )));
        }

        let mut pages = contents
            .result
            .into_iter()
            .map(|page| Page::from_api(page, referer.clone()))
            .collect::<Vec<_>>();
        pages.sort_by_key(Page::sort);
        for (expected, page) in pages.iter().enumerate() {
            if page.sort() != expected {
                return Err(ClientError::ParseError(
                    "Comici page numbers must be unique and contiguous from zero".to_owned(),
                ));
            }
        }

        Ok(Episode::new(metadata.viewer_id, metadata.title, pages))
    }

    pub async fn get_image(&self, page: &Page) -> Result<reqwest::Response, ClientError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            REFERER,
            HeaderValue::from_str(page.referer().as_str())
                .map_err(|_| ClientError::InvalidHeader)?,
        );
        self.http
            .request::<Body>(page.url().clone(), Method::GET, None, Some(headers))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        ConfigBuilder::new(Url::parse("http://localhost:4000/").unwrap()).build()
    }

    #[test]
    fn composes_contents_info_url() {
        let url = config().contents_info_url("viewer id", 12).unwrap();
        assert_eq!(url.path(), "/api/book/contentsInfo");
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![
                ("user-id".into(), "".into()),
                ("comici-viewer-id".into(), "viewer id".into()),
                ("page-from".into(), "0".into()),
                ("page-to".into(), "12".into()),
            ]
        );
    }

    #[test]
    fn parses_viewer_id_and_title() {
        let html = Html::parse_document(
            r#"<html><head><title>Series - Episode 1</title></head><body>
                <div id="comici-viewer" data-comici-viewer-id="viewer-1"></div>
            </body></html>"#,
        );
        assert_eq!(
            Client::parse_episode_html(&html).unwrap(),
            EpisodeMetadata {
                viewer_id: "viewer-1".to_owned(),
                title: "Series - Episode 1".to_owned(),
            }
        );
    }
}
