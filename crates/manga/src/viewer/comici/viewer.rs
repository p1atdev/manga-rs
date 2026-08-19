use std::collections::HashMap;

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

use super::data::{
    ContentsInfo, Episode, Page, SeriesAccessResponse, SeriesEpisode, SeriesResponse,
};

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

    fn series_range_url(
        &self,
        path: &str,
        series_id: &str,
        episode_from: usize,
        episode_to: usize,
    ) -> Result<Url, ClientError> {
        let mut url = self
            .base_url
            .join(path)
            .map_err(|_| ClientError::InvalidUrl)?;
        url.query_pairs_mut()
            .append_pair("seriesHash", series_id)
            .append_pair("episodeFrom", &episode_from.to_string())
            .append_pair("episodeTo", &episode_to.to_string());
        Ok(url)
    }

    fn series_episodes_url(
        &self,
        series_id: &str,
        episode_from: usize,
        episode_to: usize,
    ) -> Result<Url, ClientError> {
        self.series_range_url("api/episodes", series_id, episode_from, episode_to)
    }

    fn series_access_url(
        &self,
        series_id: &str,
        episode_from: usize,
        episode_to: usize,
    ) -> Result<Url, ClientError> {
        self.series_range_url("api/series/access", series_id, episode_from, episode_to)
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
    series_id: String,
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

        let series_selector = Selector::parse("a[href*='/series/']")
            .expect("the Comici series link selector is valid");
        let series_id = html
            .select(&series_selector)
            .filter_map(|element| element.attr("href"))
            .find_map(Self::series_id_from_href)
            .ok_or_else(|| ClientError::ParseError("Comici series ID was not found".to_owned()))?;

        Ok(EpisodeMetadata {
            viewer_id,
            title,
            series_id,
        })
    }

    fn series_id_from_href(href: &str) -> Option<String> {
        let path = href.split(['?', '#']).next()?.trim_matches('/');
        let segments = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        match segments.as_slice() {
            [.., "series", series_id] if !series_id.is_empty() => Some((*series_id).to_owned()),
            _ => None,
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: Url) -> Result<T, ClientError> {
        let response = self.http.get(url).await?;
        serde_json::from_slice(
            &response
                .bytes()
                .await
                .map_err(|_| ClientError::DecodeError)?,
        )
        .map_err(|error| ClientError::ParseError(error.to_string()))
    }

    async fn get_contents_info(
        &self,
        viewer_id: &str,
        page_to: usize,
    ) -> Result<ContentsInfo, ClientError> {
        self.get_json(self.config.contents_info_url(viewer_id, page_to)?)
            .await
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

    pub fn episode_url(&self, episode_id: &str) -> Result<Url, ClientError> {
        self.config.episode_url(episode_id)
    }

    pub async fn get_series_id_at(&self, episode_url: Url) -> Result<String, ClientError> {
        let html = self.get_html(episode_url).await?;
        Ok(Self::parse_episode_html(&html)?.series_id)
    }

    async fn get_series_response(
        &self,
        series_id: &str,
        episode_to: usize,
    ) -> Result<SeriesResponse, ClientError> {
        self.get_json(self.config.series_episodes_url(series_id, 1, episode_to)?)
            .await
    }

    async fn get_series_access(
        &self,
        series_id: &str,
        episode_to: usize,
    ) -> Result<SeriesAccessResponse, ClientError> {
        self.get_json(self.config.series_access_url(series_id, 1, episode_to)?)
            .await
    }

    pub async fn get_accessible_series_episodes(
        &self,
        series_id: &str,
    ) -> Result<Vec<SeriesEpisode>, ClientError> {
        let first = self.get_series_response(series_id, 1).await?;
        let total = first.series.summary.num_episodes;
        if total == 0 {
            return Err(ClientError::ParseError(
                "Comici series contains no episodes".to_owned(),
            ));
        }

        let series = if total == 1 {
            first
        } else {
            self.get_series_response(series_id, total).await?
        };
        let access = self.get_series_access(series_id, total).await?;
        Self::filter_accessible_series_episodes(series, access, total)
    }

    fn filter_accessible_series_episodes(
        series: SeriesResponse,
        access: SeriesAccessResponse,
        expected_total: usize,
    ) -> Result<Vec<SeriesEpisode>, ClientError> {
        let total = series.series.summary.num_episodes;
        if total != expected_total {
            return Err(ClientError::ParseError(
                "Comici total episode count changed while loading the series".to_owned(),
            ));
        }
        if series.series.episodes.len() != total {
            return Err(ClientError::ParseError(format!(
                "Comici returned {} episodes, expected {total}",
                series.series.episodes.len()
            )));
        }

        if access.series_access.episode_accesses.len() != total {
            return Err(ClientError::ParseError(format!(
                "Comici returned access data for {} episodes, expected {total}",
                access.series_access.episode_accesses.len()
            )));
        }

        let mut access_by_episode = HashMap::with_capacity(total);
        for episode_access in access.series_access.episode_accesses {
            if access_by_episode
                .insert(episode_access.episode_id, episode_access.has_access)
                .is_some()
            {
                return Err(ClientError::ParseError(
                    "Comici returned duplicate episode access data".to_owned(),
                ));
            }
        }

        let mut accessible = Vec::new();
        for episode in series.series.episodes {
            let has_access = access_by_episode.remove(episode.id()).ok_or_else(|| {
                ClientError::ParseError(format!(
                    "Comici access data is missing episode {}",
                    episode.id()
                ))
            })?;
            if has_access {
                accessible.push(episode);
            }
        }
        if !access_by_episode.is_empty() {
            return Err(ClientError::ParseError(
                "Comici returned access data for unknown episodes".to_owned(),
            ));
        }
        if accessible.is_empty() {
            return Err(ClientError::ParseError(
                "Comici series contains no accessible episodes".to_owned(),
            ));
        }

        Ok(accessible)
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
    fn composes_series_api_urls() {
        let config = config();
        let episodes = config.series_episodes_url("series id", 1, 42).unwrap();
        let access = config.series_access_url("series id", 1, 42).unwrap();

        assert_eq!(episodes.path(), "/api/episodes");
        assert_eq!(access.path(), "/api/series/access");
        for url in [episodes, access] {
            assert_eq!(
                url.query_pairs().collect::<Vec<_>>(),
                vec![
                    ("seriesHash".into(), "series id".into()),
                    ("episodeFrom".into(), "1".into()),
                    ("episodeTo".into(), "42".into()),
                ]
            );
        }
    }

    #[test]
    fn parses_viewer_id_and_title() {
        let html = Html::parse_document(
            r#"<html><head><title>Series - Episode 1</title></head><body>
                <div id="comici-viewer" data-comici-viewer-id="viewer-1"></div>
                <a href="/series/series-1">Series</a>
            </body></html>"#,
        );
        assert_eq!(
            Client::parse_episode_html(&html).unwrap(),
            EpisodeMetadata {
                viewer_id: "viewer-1".to_owned(),
                title: "Series - Episode 1".to_owned(),
                series_id: "series-1".to_owned(),
            }
        );
    }

    #[test]
    fn ignores_series_list_links() {
        assert_eq!(
            Client::series_id_from_href("/series/series-1"),
            Some("series-1".to_owned())
        );
        assert_eq!(
            Client::series_id_from_href("https://example.com/en/series/series-1?lang=en"),
            Some("series-1".to_owned())
        );
        assert_eq!(Client::series_id_from_href("/series/list/up/1"), None);
    }

    #[test]
    fn filters_inaccessible_episodes_and_preserves_series_order() {
        let series = serde_json::from_str(
            r#"{
                "series": {
                    "summary": {"numEpisodes": 3},
                    "episodes": [
                        {"id": "episode-1", "title": "Episode 1"},
                        {"id": "episode-2", "title": "Episode 2"},
                        {"id": "episode-3", "title": "Episode 3"}
                    ]
                }
            }"#,
        )
        .unwrap();
        let access = serde_json::from_str(
            r#"{
                "seriesAccess": {
                    "episodeAccesses": [
                        {"episodeId": "episode-3", "hasAccess": true},
                        {"episodeId": "episode-1", "hasAccess": true},
                        {"episodeId": "episode-2", "hasAccess": false}
                    ]
                }
            }"#,
        )
        .unwrap();

        let episodes = Client::filter_accessible_series_episodes(series, access, 3).unwrap();
        assert_eq!(
            episodes.iter().map(SeriesEpisode::id).collect::<Vec<_>>(),
            vec!["episode-1", "episode-3"]
        );
    }
}
