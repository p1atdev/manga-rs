use reqwest::{Body, Method, Response, header::HeaderMap};
use url::Url;

use crate::error::ClientError;

/// Shared HTTP transport used by viewer-specific clients.
#[derive(Debug, Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    default_headers: HeaderMap,
}

impl HttpClient {
    pub fn new(default_headers: HeaderMap) -> Self {
        Self {
            client: reqwest::Client::new(),
            default_headers,
        }
    }

    pub async fn request<B: Into<Body> + Send>(
        &self,
        url: Url,
        method: Method,
        body: Option<B>,
        headers: Option<HeaderMap>,
    ) -> Result<Response, ClientError> {
        let mut request = self
            .client
            .request(method, url)
            .headers(self.default_headers.clone());

        if let Some(headers) = headers {
            request = request.headers(headers);
        }
        if let Some(body) = body {
            request = request.body(body);
        }

        let response = request
            .send()
            .await
            .map_err(|_| ClientError::RequestError)?;

        if response.status().is_success() {
            Ok(response)
        } else {
            Err(ClientError::from_status(response.status()))
        }
    }

    pub async fn get(&self, url: Url) -> Result<Response, ClientError> {
        self.request::<Body>(url, Method::GET, None, None).await
    }

    pub async fn post<B: Into<Body> + Send>(
        &self,
        url: Url,
        body: B,
        headers: Option<HeaderMap>,
    ) -> Result<Response, ClientError> {
        self.request(url, Method::POST, Some(body), headers).await
    }
}
