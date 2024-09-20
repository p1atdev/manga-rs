#[cfg(feature = "fuz")]
pub mod fuz;

pub mod giga;

use std::future::Future;

use anyhow::Result;
use reqwest::{header::HeaderMap, Response};
use url::Url;

use crate::auth::Auth;

/// Manga viewer enum
pub enum Viewer {
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

    fn fetch_raw(
        &self,
        url: Url,
        method: reqwest::Method,
    ) -> impl Future<Output = Result<Response>> + Send;
}

pub trait ViewerWebsite {
    fn base_url(&self) -> Url;
}
