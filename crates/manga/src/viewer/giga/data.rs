use std::fmt;

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

use crate::data::{IndexNumber, MangaEpisode, MangaPage};

/// ChojuGiga viewer page struct
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum Page {
    Image(ImagePage),
    Other {
        #[serde(alias = "type")]
        _type: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImagePage {
    height: u32,
    width: u32,
    #[serde(alias = "src")]
    url: Url,
    #[serde(skip)]
    index: usize,
}

struct PageVisitor;

impl<'de> Visitor<'de> for PageVisitor {
    type Value = Vec<Page>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a sequence of pages")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Vec<Page>, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut pages = Vec::new();
        let mut index = 0;
        while let Some(page) = seq.next_element::<Page>()? {
            if let Page::Image(mut image_page) = page {
                image_page.index = index;
                pages.push(Page::Image(image_page));
                index += 1;
            }
        }
        Ok(pages)
    }
}

fn deserialize_pages_with_indices<'de, D>(deserializer: D) -> Result<Vec<Page>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_seq(PageVisitor)
}

impl Page {
    pub fn url(&self) -> Result<Url> {
        match self {
            Page::Image(ImagePage { url, .. }) => Ok(url.clone()),
            _ => bail!("Page is not an image"),
        }
    }
}

impl MangaPage for Page {
    fn index(&self) -> Result<usize> {
        match self {
            Page::Image(ImagePage { index, .. }) => Ok(*index),
            _ => bail!("Page is not an image"),
        }
    }

    fn is_image(&self) -> bool {
        matches!(self, Page::Image(_))
    }
}

/// ChojuGiga viewer episode struct
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Episode {
    #[serde(alias = "readableProduct", rename_all = "camelCase")]
    ReadableProduct {
        id: String,
        title: String,
        type_name: String, // episode
        is_public: bool,
        #[serde(alias = "nextReadableProductUri")]
        next_episode_url: Option<Url>,
        #[serde(alias = "number")]
        index: IndexNumber,
        page_structure: Option<EpisodePageStructure>,
        #[serde(alias = "permalink")]
        url: Url,
        published_at: Option<DateTime<Utc>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EpisodePageStructure {
    #[serde(alias = "choJuGiga")]
    choju_giga: String, // baku
    reading_direction: ReadingDirection,
    start_position: Option<String>,
    #[serde(deserialize_with = "deserialize_pages_with_indices")]
    pages: Vec<Page>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ReadingDirection {
    #[serde(alias = "rtl")]
    RightToLeft,
    #[serde(alias = "ltr")]
    LeftToRight,
    #[serde(alias = "ttb")]
    TopToBottom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeSeriesInfo {
    id: String,
    title: String,
    thumbnail_url: Url,
    #[serde(alias = "subThumbnailSquare")]
    thumbnail_url_square: Url,
}

impl Episode {
    pub fn url(&self) -> Url {
        match self {
            Episode::ReadableProduct { url, .. } => url.clone(),
        }
    }
}

impl MangaEpisode<Page> for Episode {
    fn id(&self) -> String {
        match self {
            Episode::ReadableProduct { id, .. } => id.clone(),
        }
    }

    fn index(&self) -> IndexNumber {
        match self {
            Episode::ReadableProduct { index, .. } => index.clone(),
        }
    }

    fn title(&self) -> Option<String> {
        match self {
            Episode::ReadableProduct { title, .. } => Some(title.clone()),
        }
    }

    fn pages(&self) -> &[Page] {
        match self {
            Episode::ReadableProduct { page_structure, .. } => {
                if let Some(EpisodePageStructure { pages, .. }) = page_structure {
                    pages
                } else {
                    &[]
                }
            }
        }
    }

    fn into_pages(self) -> Vec<Page> {
        match self {
            Episode::ReadableProduct { page_structure, .. } => page_structure
                .map(|structure| structure.pages)
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    id: String,
    title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gtm {
    episode: GtmEpisode,
}

impl Gtm {
    pub fn episode(&self) -> &GtmEpisode {
        &self.episode
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GtmEpisode {
    magazine_label: Option<serde_json::Value>,
    episode_id: String,
    magazine_label_id: Option<serde_json::Value>,
    readable_product_id: String,
    episode_title: String,
    content_id: String,
    series_id: String,
    henshubu: Option<serde_json::Value>,
    series_title: String,
    series_ongoing: i64,
    magazine_label_title: Option<serde_json::Value>,
}

impl GtmEpisode {
    pub fn episode_id(&self) -> &str {
        &self.episode_id
    }

    pub fn readable_product_id(&self) -> &str {
        &self.readable_product_id
    }

    pub fn episode_title(&self) -> &str {
        &self.episode_title
    }

    pub fn content_id(&self) -> &str {
        &self.content_id
    }

    pub fn series_id(&self) -> &str {
        &self.series_id
    }

    pub fn series_title(&self) -> &str {
        &self.series_title
    }

    pub fn series_ongoing(&self) -> i64 {
        self.series_ongoing
    }
}
