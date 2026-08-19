pub mod detect;
#[cfg(feature = "fuz")]
pub mod fuz;

#[cfg(feature = "giga")]
pub mod giga;
#[cfg(feature = "kadokomi")]
pub mod kadocomi;
#[cfg(feature = "mangaz")]
pub mod mangaz;

use std::future::Future;

use anyhow::Result;
use reqwest::{Response, StatusCode, header::HeaderMap};
use url::Url;

use crate::{auth::Auth, error::ClientError};

/// Manga viewer implementation detected from page contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ViewerType {
    #[cfg(feature = "giga")]
    Giga,
    #[cfg(feature = "kadokomi")]
    Kadokomi,
    #[cfg(feature = "fuz")]
    Fuz,
    #[cfg(feature = "mangaz")]
    MangaZ,
}

pub trait ViewerConfig {
    fn create_header(&self) -> Result<HeaderMap, ClientError>;
}

pub trait ViewerConfigBuilder<V: ViewerConfig, A: Auth> {
    /// Set auth configuration
    fn set_auth(&mut self, auth: A) -> &mut Self;

    fn build(&self) -> V;
}

pub trait ViewerClient<V: ViewerConfig> {
    fn new(config: V) -> Self;

    fn fetch_raw<B: Into<reqwest::Body> + Send>(
        &self,
        url: Url,
        method: reqwest::Method,
        body: Option<B>,
        headers: Option<HeaderMap>,
    ) -> impl Future<Output = Result<Response, ClientError>> + Send;

    /// simple GET request
    fn get(
        &self,
        url: Url,
    ) -> impl std::future::Future<Output = Result<Response, ClientError>> + Send {
        self.fetch_raw::<reqwest::Body>(url, reqwest::Method::GET, None, None)
    }

    /// simple POST request
    fn post<B: Into<reqwest::Body> + Send>(
        &self,
        url: Url,
        body: B,
        headers: Option<HeaderMap>,
    ) -> impl std::future::Future<Output = Result<Response, ClientError>> + Send {
        self.fetch_raw::<reqwest::Body>(url, reqwest::Method::POST, Some(body.into()), headers)
    }

    fn map_error_status(&self, status: StatusCode) -> ClientError {
        ClientError::from_status(status)
    }
}
