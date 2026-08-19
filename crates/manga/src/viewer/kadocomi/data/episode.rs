use anyhow::Result;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use url::Url;

use crate::data::{IndexNumber, MangaEpisode, MangaPage};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    promotions_end: Vec<Option<serde_json::Value>>,
    label_logo: String,
    scroll_direction: String,
    expires_at: String,
    start_position: String,
    display_ads: bool,
    #[serde(
        rename = "manuscripts",
        deserialize_with = "deserialize_pages_with_indices"
    )]
    pages: Vec<Page>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    drm_mode: DrmMode,
    drm_hash: String, // xor key
    drm_image_url: String,
    page: i64,
    width: i64,
    height: i64,
    #[serde(skip)]
    index: usize,
}

impl Page {
    pub fn url(&self) -> Result<Url> {
        Ok(Url::parse(&self.drm_image_url)?)
    }

    pub fn encryption_mode(&self) -> DrmMode {
        self.drm_mode.clone()
    }

    pub fn encryption_key(&self) -> String {
        self.drm_hash.clone()
    }
}

struct PageVisitor;

impl<'de> Visitor<'de> for PageVisitor {
    type Value = Vec<Page>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a sequence of pages")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Vec<Page>, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut pages = Vec::new();
        while let Some(mut page) = seq.next_element::<Page>()? {
            page.index = pages.len();
            pages.push(page);
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrmMode {
    Xor,
    Raw,
}

impl MangaPage for Page {
    fn index(&self) -> Result<usize> {
        Ok(self.index)
    }

    fn is_image(&self) -> bool {
        true
    }
}

impl MangaEpisode<Page> for Episode {
    fn id(&self) -> String {
        self.label_logo.clone()
    }

    fn index(&self) -> IndexNumber {
        IndexNumber::Int(self.pages.len())
    }

    fn title(&self) -> Option<String> {
        None
    }

    fn pages(&self) -> Vec<Page> {
        self.pages.clone()
    }
}
