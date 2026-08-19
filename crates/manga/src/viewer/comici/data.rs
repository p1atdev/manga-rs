use std::fmt;

use serde::{Deserialize, Deserializer, de};
use url::Url;

use crate::data::{IndexNumber, MangaEpisode, MangaPage};

pub const GRID_SIZE: usize = 4;
pub const TILE_COUNT: usize = GRID_SIZE * GRID_SIZE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scramble([usize; TILE_COUNT]);

impl Scramble {
    pub fn tile_sources(&self) -> &[usize; TILE_COUNT] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Scramble {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ScrambleVisitor;

        impl<'de> de::Visitor<'de> for ScrambleVisitor {
            type Value = Scramble;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON-encoded permutation of the 16 Comici tiles")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let sources: Vec<usize> = serde_json::from_str(value).map_err(E::custom)?;
                let sources: [usize; TILE_COUNT] =
                    sources.try_into().map_err(|sources: Vec<_>| {
                        E::custom(format!(
                            "Comici scramble must contain {TILE_COUNT} entries, got {}",
                            sources.len()
                        ))
                    })?;

                let mut seen = [false; TILE_COUNT];
                for source in sources {
                    if source >= TILE_COUNT {
                        return Err(E::custom(format!(
                            "Comici scramble tile index {source} is out of range"
                        )));
                    }
                    if seen[source] {
                        return Err(E::custom(format!(
                            "Comici scramble tile index {source} occurs more than once"
                        )));
                    }
                    seen[source] = true;
                }

                Ok(Scramble(sources))
            }
        }

        deserializer.deserialize_str(ScrambleVisitor)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContentsInfo {
    pub(super) total_pages: usize,
    pub(super) result: Vec<ApiPage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ApiPage {
    image_url: Url,
    scramble: Scramble,
    sort: usize,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    image_url: Url,
    scramble: Scramble,
    sort: usize,
    width: u32,
    height: u32,
    referer: Url,
}

impl Page {
    pub(super) fn from_api(page: ApiPage, referer: Url) -> Self {
        Self {
            image_url: page.image_url,
            scramble: page.scramble,
            sort: page.sort,
            width: page.width,
            height: page.height,
            referer,
        }
    }

    pub fn url(&self) -> &Url {
        &self.image_url
    }

    pub fn scramble(&self) -> &Scramble {
        &self.scramble
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn sort(&self) -> usize {
        self.sort
    }

    pub(super) fn referer(&self) -> &Url {
        &self.referer
    }
}

impl MangaPage for Page {
    fn index(&self) -> anyhow::Result<usize> {
        Ok(self.sort)
    }

    fn is_image(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    id: String,
    title: String,
    pages: Vec<Page>,
}

impl Episode {
    pub(super) fn new(id: String, title: String, pages: Vec<Page>) -> Self {
        Self { id, title, pages }
    }
}

impl MangaEpisode<Page> for Episode {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn index(&self) -> IndexNumber {
        IndexNumber::String(self.id.clone())
    }

    fn title(&self) -> Option<String> {
        Some(self.title.clone())
    }

    fn pages(&self) -> &[Page] {
        &self.pages
    }

    fn into_pages(self) -> Vec<Page> {
        self.pages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_contents_info_and_scramble_string() {
        let contents: ContentsInfo = serde_json::from_str(
            r#"{
                "totalPages": 1,
                "result": [{
                    "imageUrl": "https://viewer.example/book/page.jpg",
                    "scramble": "[13,0,7,10,1,8,12,5,15,14,2,9,11,4,6,3]",
                    "sort": 0,
                    "width": 850,
                    "height": 1200
                }]
            }"#,
        )
        .unwrap();

        assert_eq!(contents.total_pages, 1);
        assert_eq!(contents.result[0].scramble.tile_sources()[0], 13);
    }

    #[test]
    fn rejects_non_permutation_scramble() {
        let result =
            serde_json::from_str::<Scramble>(r#""[0,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14]""#);
        assert!(result.is_err());
    }
}
