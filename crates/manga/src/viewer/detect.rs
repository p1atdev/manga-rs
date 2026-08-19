use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
#[cfg(any(feature = "comici", feature = "giga", feature = "kadokomi"))]
use scraper::{Html, Selector};
use thiserror::Error;
use url::Url;

use crate::{error::ClientError, http::HttpClient, utils::UserAgent, viewer::ViewerType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedViewer {
    viewer_type: ViewerType,
    base_url: Url,
}

impl DetectedViewer {
    pub fn viewer_type(&self) -> ViewerType {
        self.viewer_type
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }
}

#[derive(Debug, Error)]
pub enum DetectionError {
    #[error(transparent)]
    Client(#[from] ClientError),

    #[error("only HTTP and HTTPS URLs can be inspected: {0}")]
    UnsupportedScheme(String),

    #[error("the page does not contain a supported manga viewer signature")]
    UnsupportedViewer,
}

pub async fn detect(url: &Url) -> Result<DetectedViewer, DetectionError> {
    base_url(url)?;
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(UserAgent::Bot.value()));
    let response = HttpClient::new(headers).get(url.clone()).await?;
    let base_url = base_url(response.url())?;
    let html = response
        .text()
        .await
        .map_err(|_| ClientError::DecodeError)?;

    Ok(DetectedViewer {
        viewer_type: detect_html(&html)?,
        base_url,
    })
}

pub fn detect_html(html: &str) -> Result<ViewerType, DetectionError> {
    #[cfg(any(feature = "comici", feature = "giga", feature = "kadokomi"))]
    let document = Html::parse_document(html);
    let _ = html;

    #[cfg(feature = "comici")]
    {
        let selector = Selector::parse("div#comici-viewer[data-comici-viewer-id]")
            .expect("the Comici viewer selector is valid");
        if document.select(&selector).next().is_some() {
            return Ok(ViewerType::Comici);
        }
    }

    #[cfg(feature = "giga")]
    {
        let selector = Selector::parse("script#episode-json[data-value]")
            .expect("the Giga viewer selector is valid");
        if let Some(json) = document
            .select(&selector)
            .next()
            .and_then(|element| element.attr("data-value"))
        {
            let is_giga = serde_json::from_str::<serde_json::Value>(json)
                .ok()
                .and_then(|value| value.get("readableProduct").cloned())
                .is_some_and(|episode| {
                    episode.get("id").is_some_and(serde_json::Value::is_string)
                        && episode.get("pageStructure").is_some()
                });
            if is_giga {
                return Ok(ViewerType::Giga);
            }
        }
    }

    #[cfg(feature = "kadokomi")]
    {
        let selector =
            Selector::parse("script#__NEXT_DATA__").expect("the Next.js selector is valid");
        if let Some(script) = document.select(&selector).next() {
            let value = serde_json::from_str::<serde_json::Value>(&script.inner_html())
                .map_err(|_| DetectionError::UnsupportedViewer)?;
            let has_work_code = value
                .pointer("/props/pageProps/workCode")
                .is_some_and(serde_json::Value::is_string);
            let has_episode = value
                .pointer("/props/pageProps/dehydratedState/queries")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|queries| {
                    queries.iter().any(|query| {
                        query.pointer("/state/data/episode").is_some_and(|episode| {
                            episode.get("id").is_some_and(serde_json::Value::is_string)
                                && episode
                                    .get("title")
                                    .is_some_and(serde_json::Value::is_string)
                        })
                    })
                });
            let is_kadokomi = has_work_code && has_episode;
            if is_kadokomi {
                return Ok(ViewerType::Kadokomi);
            }
        }
    }

    Err(DetectionError::UnsupportedViewer)
}

fn base_url(url: &Url) -> Result<Url, DetectionError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(DetectionError::UnsupportedScheme(url.scheme().to_owned()));
    }

    let mut base_url = url.clone();
    base_url.set_path("/");
    base_url.set_query(None);
    base_url.set_fragment(None);
    Ok(base_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "comici")]
    #[test]
    fn detects_comici_from_viewer_container() {
        let html = r#"<div id="comici-viewer" data-comici-viewer-id="viewer-id"></div>"#;
        assert_eq!(detect_html(html).unwrap(), ViewerType::Comici);
    }

    #[cfg(feature = "giga")]
    #[test]
    fn detects_giga_from_episode_script() {
        let html = r#"<script id="episode-json" data-value='{"readableProduct":{"id":"1","pageStructure":null}}'></script>"#;
        assert_eq!(detect_html(html).unwrap(), ViewerType::Giga);
    }

    #[cfg(feature = "giga")]
    #[test]
    fn rejects_unrelated_episode_script() {
        let html = r#"<script id="episode-json" data-value="{}"></script>"#;
        assert!(matches!(
            detect_html(html),
            Err(DetectionError::UnsupportedViewer)
        ));
    }

    #[cfg(feature = "kadokomi")]
    #[test]
    fn detects_kadokomi_from_next_data_shape() {
        let html = r#"
            <script id="__NEXT_DATA__">
              {"props":{"pageProps":{"workCode":"work-id","dehydratedState":{"queries":[
                {"state":{"data":{"episode":{"id":"episode-id","title":"Episode"}}}}
              ]}}}}
            </script>
        "#;
        assert_eq!(detect_html(html).unwrap(), ViewerType::Kadokomi);
    }

    #[test]
    fn rejects_generic_next_application() {
        let html = r#"<script id="__NEXT_DATA__">{"props":{}}</script>"#;
        assert!(matches!(
            detect_html(html),
            Err(DetectionError::UnsupportedViewer)
        ));
    }

    #[test]
    fn keeps_scheme_host_and_port_for_custom_site() {
        let url = Url::parse("http://localhost:4321/episode/123?preview=1").unwrap();
        assert_eq!(base_url(&url).unwrap().as_str(), "http://localhost:4321/");
    }
}
