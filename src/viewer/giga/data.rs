use std::fmt;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

use crate::data::{MangaEpisode, MangaPage};

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
        while let Some(mut page) = seq.next_element::<Page>()? {
            match page {
                Page::Image(ref mut image_page) => {
                    pages.push(Page::Image(ImagePage {
                        height: image_page.height,
                        width: image_page.width,
                        url: image_page.url.clone(),
                        index: index,
                    }));
                    index += 1;
                }
                _ => {}
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

impl MangaPage for Page {
    fn index(&self) -> Result<usize> {
        match self {
            Page::Image(ImagePage { index, .. }) => Ok(*index),
            _ => bail!("Page is not an image"),
        }
    }

    fn url(&self) -> Result<Url> {
        match self {
            Page::Image(ImagePage { url, .. }) => Ok(url.clone()),
            _ => bail!("Page is not an image"),
        }
    }

    fn is_image(&self) -> bool {
        match self {
            Page::Image(_) => true,
            _ => false,
        }
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
        index: usize,
        page_structure: EpisodePageStructure,
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

impl MangaEpisode<Page> for Episode {
    fn id(&self) -> String {
        match self {
            Episode::ReadableProduct { id, .. } => id.clone(),
        }
    }

    fn index(&self) -> usize {
        match self {
            Episode::ReadableProduct { index, .. } => *index,
        }
    }

    fn title(&self) -> Option<String> {
        match self {
            Episode::ReadableProduct { title, .. } => Some(title.clone()),
        }
    }

    fn date(&self) -> Option<DateTime<Utc>> {
        match self {
            Episode::ReadableProduct { published_at, .. } => *published_at,
        }
    }

    fn url(&self) -> Url {
        match self {
            Episode::ReadableProduct { url, .. } => url.clone(),
        }
    }

    fn pages(&self) -> Vec<Page> {
        match self {
            Episode::ReadableProduct {
                page_structure: EpisodePageStructure { pages, .. },
                ..
            } => pages.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    id: String,
    title: String,
}
