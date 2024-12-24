use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;
use reqwest::header::{HeaderName, HeaderValue};

use crate::{auth::Auth, viewer::ViewerWebsite};

use super::viewer::Website;

/// Common auth key for Ichijin.
/// Maybe this will not be changed soon.
static ENV_KEY: &str = "GGXGejnSsZw-IxHKQp8OQKHH-NDItSbEq5PU0g2w1W4=";

pub struct StaticAuth {
    key: &'static str,
}

impl StaticAuth {
    pub fn new() -> Self {
        Self { key: ENV_KEY }
    }
}

impl Auth for StaticAuth {
    fn get_header_key() -> HeaderName {
        HeaderName::from_static("X-API-Environment-Key")
    }

    fn get_header_value(&self) -> Result<HeaderValue> {
        Ok(HeaderValue::from_str(self.key)?)
    }
}

// this matches to the environment key in the js script
static ENV_KEY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"="((?:[^G]|G[^-])[A-Za-z0-9\-\+/_]*={0,2})""#).unwrap());

pub struct DynamicAuth {
    key: String,
}

impl DynamicAuth {
    async fn get_key() -> Result<String> {
        // 1. get top page
        let html = reqwest::get(Website::Ichijin.base_url())
            .await?
            .text()
            .await?;
        // html might be minimized

        // 2. get the target script url
        let script_path = html
            .split("/_next/static/chunks/pages/_app-")
            .collect::<Vec<_>>()
            .get(1)
            .context("Failed to find the script url")?
            .split('"')
            .next()
            .context("Failed to find the script url")?;
        let script_url = Website::Ichijin
            .base_url()
            .join(&format!("/_next/static/chunks/pages/_app-{}", script_path))?;

        // 3. get the script
        let script = reqwest::get(script_url).await?.text().await?;

        // 4. get the key
        let script_part = script
            .split(r#""https://api.ichijin-plus.com""#)
            .collect::<Vec<_>>()
            .get(1)
            .context("Failed to find the script part")?
            .split('}')
            .next()
            .context("Failed to find the script part")?;

        println!("{}", script_part);

        // 5. extract the key using regex
        let key = ENV_KEY_PATTERN
            .captures(script_part)
            .context("Failed to extract the key")?
            .get(1)
            .context("Failed to extract the key")?
            .as_str()
            .to_string();

        Ok(key)
    }

    pub async fn new() -> Result<Self> {
        Ok(Self {
            key: Self::get_key().await?,
        })
    }
}

impl Auth for DynamicAuth {
    fn get_header_key() -> HeaderName {
        HeaderName::from_static("X-API-Environment-Key")
    }

    fn get_header_value(&self) -> Result<HeaderValue> {
        Ok(HeaderValue::from_str(&self.key)?)
    }
}

#[cfg(test)]
mod test {

    use anyhow::Result;

    use super::*;

    #[tokio::test]
    async fn test_env_key_pattern() -> Result<()> {
        let key = "VGhpcyBpcyBhIHNhbXBsZSBrZXk=";
        let sample = format!(
            r#",o="G-0BXBCCCXYZ",s="https://ichijin-plus.com",a="{}""#,
            key
        );

        let extracted = ENV_KEY_PATTERN
            .captures(&sample)
            .ok_or_else(|| anyhow::anyhow!("Failed to extract the key"))?
            .get(1)
            .ok_or_else(|| anyhow::anyhow!("Failed to get 1"))?
            .as_str();

        assert_eq!(extracted, key);

        Ok(())
    }

    #[tokio::test]
    async fn test_dynamic_auth() -> Result<()> {
        let auth = DynamicAuth::new().await?;
        let _ = auth.get_header_value()?;

        Ok(())
    }
}
