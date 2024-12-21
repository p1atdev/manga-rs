use anyhow::Result;
use feed_rs::{model::Feed as FeedOutput, parser};

pub enum FeedContent {
    FeedRs(FeedOutput),
}

pub struct FeedParser {}

impl FeedParser {
    pub fn new() -> Self {
        FeedParser {}
    }

    pub fn parse<T: AsRef<[u8]>>(&self, buffer: &T) -> Result<FeedContent> {
        let feed = parser::parse(buffer.as_ref())?;

        Ok(FeedContent::FeedRs(feed))
    }
}
