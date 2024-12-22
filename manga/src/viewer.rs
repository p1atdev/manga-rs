#[cfg(feature = "fuz")]
pub mod fuz;

pub mod giga;
pub mod kadocomi;
#[deprecated]
pub mod mangaz;

use std::future::Future;

use anyhow::Result;
use reqwest::{header::HeaderMap, IntoUrl, Response, StatusCode};
use url::Url;

use crate::{
    auth::Auth,
    error::{ClientError, HttpError},
};

/// Manga viewer enum
pub enum ViewerType {
    Giga,
    Ichijin,
    #[cfg(feature = "fuz")]
    Fuz,
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
        match status {
            StatusCode::NOT_FOUND => ClientError::HttpError(HttpError::PageNotFound),
            StatusCode::TOO_MANY_REQUESTS => ClientError::HttpError(HttpError::TooManyRequests),
            StatusCode::INTERNAL_SERVER_ERROR => {
                ClientError::HttpError(HttpError::InternalServerError)
            }
            StatusCode::BAD_REQUEST => ClientError::HttpError(HttpError::BadRequest),
            StatusCode::UNAUTHORIZED => ClientError::HttpError(HttpError::Unauthorized),
            StatusCode::FORBIDDEN => ClientError::HttpError(HttpError::Forbidden),
            _ => ClientError::HttpError(HttpError::Unknown(
                status.canonical_reason().unwrap_or("").to_string(),
            )),
        }
    }
}

pub trait ViewerWebsite<T> {
    fn host(&self) -> &str;
    fn base_url(&self) -> Url;
    fn lookup(host: &str) -> Option<T>;
}
