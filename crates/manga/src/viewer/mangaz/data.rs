use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::data::{IndexNumber, MangaEpisode, MangaPage};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Episode {
    #[serde(rename = "verkey")]
    verkey: String,
    #[serde(rename = "CACHE_VERSION")]
    cache_version: i64,
    location: Location,
    book: Book,
    authors: HashMap<String, Author>,
    images: Vec<Option<serde_json::Value>>,
    #[serde(rename = "Orders")]
    pages: Vec<Page>,
    user: User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    id: i64,
    position: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    baid: i64,
    series_id: i64,
    status: String,
    reading: i64,
    uploaded_by: Option<serde_json::Value>,
    title: String,
    description: Option<String>,
    site_target: i64,
    r18: i64,
    volume: IndexNumber,
    url: String,
    thumb_image: String,
    thumb100: String,
    thumb160: String,
    thumb240: String,
    thumb100_webp: String,
    thumb160_webp: String,
    thumb240_webp: String,
    cover_image: String,
    publication: String,
    pdf_sale: i64,
    vw_share: i64,
    category: String,
    rating: i64,
    premium: i64,
    pages: i64,
    trial_pages: Option<serde_json::Value>,
    #[serde(rename = "Authors")]
    authors: Vec<String>,
    page_layout: String,
    page_direction: i64,
    page_max_width: i64,
    page_max_height: i64,
    image_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    viewer: String,
    base: String,
    scramble_dir: String,
    domain: String,
    host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub no: i64,
    pub name: String,
    pub side: Side,
    pub pair_no: Option<i64>,
    pub scramble: Scramble,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scramble {
    #[serde(rename = "w")]
    pub width: u32,
    #[serde(rename = "h")]
    pub height: u32,
    pub crops: Vec<Crop>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crop {
    #[serde(rename = "x")]
    pub original_x: u32,
    #[serde(rename = "y")]
    pub original_y: u32,
    #[serde(rename = "x2")]
    pub scrambled_x: u32,
    #[serde(rename = "y2")]
    pub scrambled_y: u32,
    #[serde(rename = "w")]
    pub width: u32,
    #[serde(rename = "h")]
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    login: bool,
    premium: bool,
    initial: InitialPage,
    trial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialPage {
    image_no: i64,
}

impl Page {
    pub fn image_url(&self, base_url: &Url, verkey: &str) -> Result<Url> {
        let mut url = base_url.join(&self.name)?;
        url.set_query(Some(format!("{}=", verkey).as_str()));
        Ok(url)
    }
}

impl MangaPage for Page {
    fn index(&self) -> Result<usize> {
        Ok(self.no as usize)
    }

    fn is_image(&self) -> bool {
        true
    }
}

impl Episode {
    pub fn verkey(&self) -> &str {
        &self.verkey
    }

    pub fn image_base_url(&self) -> Result<Url> {
        let url = Url::parse(&self.location.base)?
            .join(format!("{}/", &self.location.scramble_dir).as_str())?;
        Ok(url)
    }
}

impl MangaEpisode<Page> for Episode {
    fn id(&self) -> String {
        self.book.baid.to_string()
    }

    fn index(&self) -> IndexNumber {
        self.book.volume.clone()
    }

    fn title(&self) -> Option<String> {
        Some(self.book.title.clone())
    }

    fn pages(&self) -> &[Page] {
        &self.pages
    }

    fn into_pages(self) -> Vec<Page> {
        self.pages
    }
}
