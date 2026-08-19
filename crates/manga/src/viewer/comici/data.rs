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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct SeriesResponse {
    pub(super) series: SeriesInfo,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct SeriesInfo {
    pub(super) summary: SeriesSummary,
    pub(super) episodes: Vec<SeriesEpisode>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct SeriesSummary {
    pub(super) num_episodes: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SeriesEpisode {
    id: String,
    title: String,
}

impl SeriesEpisode {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct SeriesAccessResponse {
    pub(super) series_access: SeriesAccess,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct SeriesAccess {
    pub(super) episode_accesses: Vec<EpisodeAccess>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct EpisodeAccess {
    pub(super) episode_id: String,
    pub(super) has_access: bool,
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
    series_title: String,
    pages: Vec<Page>,
}

impl Episode {
    pub(super) fn new(id: String, title: String, series_title: String, pages: Vec<Page>) -> Self {
        Self {
            id,
            title,
            series_title,
            pages,
        }
    }

    pub fn series_title(&self) -> &str {
        &self.series_title
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

    #[test]
    fn parses_series_and_access_responses() {
        let series: SeriesResponse = serde_json::from_str(
            r#"{
                "series": {
                    "summary": {"numEpisodes": 2},
                    "episodes": [
                        {"id": "episode-1", "title": "Episode 1"},
                        {"id": "episode-2", "title": "Episode 2"}
                    ]
                }
            }"#,
        )
        .unwrap();
        let access: SeriesAccessResponse = serde_json::from_str(
            r#"{
                "seriesAccess": {
                    "episodeAccesses": [
                        {"episodeId": "episode-1", "hasAccess": true},
                        {"episodeId": "episode-2", "hasAccess": false}
                    ]
                }
            }"#,
        )
        .unwrap();

        assert_eq!(series.series.summary.num_episodes, 2);
        assert_eq!(series.series.episodes[1].title(), "Episode 2");
        assert!(access.series_access.episode_accesses[0].has_access);
    }
}
