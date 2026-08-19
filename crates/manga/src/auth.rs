use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::header::{self, HeaderName, HeaderValue};

/// Auth trait
pub trait Auth {
    /// create header key
    fn get_header_key() -> HeaderName {
        header::AUTHORIZATION
    }

    /// create header value
    fn get_header_value(&self) -> Result<HeaderValue>;
}

/// Basic auth
#[derive(Debug, Clone)]
pub struct BasicAuth {
    username: String,
    password: String,
}

impl BasicAuth {
    /// create new basic auth
    pub fn new(username: &str, password: &str) -> Self {
        Self {
            username: username.to_string(),
            password: password.to_string(),
        }
    }
}

impl Auth for BasicAuth {
    fn get_header_value(&self) -> Result<HeaderValue> {
        let value = format!(
            "Basic {}",
            STANDARD.encode(format!("{}:{}", self.username, self.password))
        );
        Ok(HeaderValue::from_str(&value)?)
    }

    // fn get_header_value(&self) -> String {
    //     format!("{}:{}", self.username, self.password)
    // }
}

/// Bearer auth
#[derive(Debug, Clone)]
pub struct BearerAuth {
    token: String,
}

impl BearerAuth {
    /// create new bearer auth
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
        }
    }
}

impl Auth for BearerAuth {
    fn get_header_value(&self) -> Result<HeaderValue> {
        let value = format!("Bearer {}", self.token);
        Ok(HeaderValue::from_str(&value)?)
    }

    // fn get_header_value(&self) -> String {
    //     self.token.clone()
    // }
}

/// Empty auth. do nothing
#[derive(Debug, Clone)]
pub struct EmptyAuth {}

impl Auth for EmptyAuth {
    fn get_header_value(&self) -> Result<HeaderValue> {
        Ok(HeaderValue::from_str("")?)
    }

    // fn get_header_value(&self) -> String {
    //     "".to_string()
    // }
}
