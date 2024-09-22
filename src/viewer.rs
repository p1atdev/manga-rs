#[cfg(feature = "fuz")]
pub mod fuz;

pub mod giga;

use std::future::Future;

use anyhow::Result;
use reqwest::{header::HeaderMap, Response};
use url::Url;

use crate::auth::Auth;

/// Manga viewer enum
pub enum ViewerType {
    Giga,
    Ichijin,
    #[cfg(feature = "fuz")]
    Fuz,
}

pub trait ViewerConfig {
    fn create_header(&self) -> Result<HeaderMap>;
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
    ) -> impl Future<Output = Result<Response>> + Send;

    /// simple GET request
    fn get(&self, url: Url) -> impl std::future::Future<Output = Result<Response>> + Send {
        self.fetch_raw::<reqwest::Body>(url, reqwest::Method::GET, None, None)
    }

    /// simple POST request
    fn post<B: Into<reqwest::Body> + Send>(
        &self,
        url: Url,
        body: B,
        headers: Option<HeaderMap>,
    ) -> impl std::future::Future<Output = Result<Response>> + Send {
        self.fetch_raw::<reqwest::Body>(url, reqwest::Method::POST, Some(body.into()), headers)
    }

    /// Parse episode id from url
    fn parse_episode_id(&self, url: &Url) -> Option<String>;
}

pub trait ViewerWebsite<T> {
    fn host(&self) -> &str;
    fn base_url(&self) -> Url;
    fn lookup(host: &str) -> Option<T>;
}
