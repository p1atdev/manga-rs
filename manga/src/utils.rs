const BOT_USER_AGENT: &str = "manga.rs/1.0";

pub enum UserAgent {
    Bot,
}

impl UserAgent {
    pub fn value(&self) -> String {
        match self {
            UserAgent::Bot => BOT_USER_AGENT,
        }
        .to_string()
    }
}

/// Include generated proto files
macro_rules! include_proto {
    ($name:literal) => {
        include!(concat!(env!("OUT_DIR"), "/", $name, ".rs"));
    };
}
use std::io::Cursor;

use anyhow::{anyhow, Result};
use image::{DynamicImage, ImageFormat};
pub(crate) use include_proto;
use scraper::Html;

pub(crate) type Bytes = Vec<u8>;

pub(crate) fn encode_image(image: &DynamicImage, format: ImageFormat) -> Result<Bytes> {
    let mut buffer = Vec::new();
    image.write_to(&mut Cursor::new(&mut buffer), format)?;
    Ok(buffer)
}

/// Get script#__NEXT_DATA__ from HTML
pub(crate) fn extract_next_data_json(html: &Html) -> Result<String> {
    let script = html
        .select(&scraper::Selector::parse("script#__NEXT_DATA__").unwrap())
        .next()
        .ok_or_else(|| anyhow!("script#__NEXT_DATA__ not found"))?;
    let inner_html = script.inner_html();

    Ok(inner_html)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_extract_next_data_json() {
        let html = Html::parse_document(
            r#"
            <html>
                <head>
                    <script id="__NEXT_DATA__">
                        {"key": "value"}
                    </script>
                </head>
            </html>
        "#,
        );

        let json = extract_next_data_json(&html).unwrap();
        assert_eq!(json.trim(), r#"{"key": "value"}"#);
    }
}
