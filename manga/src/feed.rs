use anyhow::Result;
use chrono::{DateTime, Utc};
use feed_rs::{
    model::{Entry, Feed as FeedOutput, Link},
    parser,
};

#[derive(Clone, Debug)]
pub struct FeedContent(FeedOutput);
#[derive(Clone, Debug)]
pub struct FeedEntry(Entry);
#[derive(Clone, Debug)]
pub struct FeedLink(Link);

impl FeedContent {
    pub fn entries(&self) -> Vec<FeedEntry> {
        self.0
            .entries
            .iter()
            .map(|entry| FeedEntry(entry.clone()))
            .collect()
    }
}

impl FeedEntry {
    pub fn title(&self) -> Option<String> {
        self.0
            .title
            .clone()
            .map_or_else(|| None, |title| Some(title.content))
    }

    pub fn links(&self) -> Vec<FeedLink> {
        self.0
            .links
            .iter()
            .map(|link| FeedLink(link.clone()))
            .collect()
    }

    pub fn updated(&self) -> Option<DateTime<Utc>> {
        self.0.updated
    }
}

impl FeedLink {
    pub fn url(&self) -> String {
        self.0.href.clone()
    }

    pub fn rel(&self) -> Option<String> {
        self.0.rel.clone()
    }

    pub fn title(&self) -> Option<String> {
        self.0.title.clone()
    }

    pub fn href_lang(&self) -> Option<String> {
        self.0.href_lang.clone()
    }

    pub fn media_type(&self) -> Option<String> {
        self.0.media_type.clone()
    }

    pub fn length(&self) -> Option<u64> {
        self.0.length
    }
}

#[derive(Clone, Debug)]
pub struct FeedParser {}

impl FeedParser {
    pub fn new() -> Self {
        FeedParser {}
    }

    pub fn parse<T: AsRef<[u8]>>(&self, buffer: &T) -> Result<FeedContent> {
        let feed = parser::parse(buffer.as_ref())?;

        Ok(FeedContent(feed))
    }
}
