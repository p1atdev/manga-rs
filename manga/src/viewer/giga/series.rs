use std::path::Path;

use anyhow::{Context, Result};
use futures::{stream, StreamExt, TryStreamExt};
use url::Url;

use crate::{
    data::MangaEpisode,
    error::PipelineError,
    pipeline::{EpisodePipeline, EpisodeQueueItem, SeriesPipeline},
};

use super::{
    data::{Episode, Page},
    pipeline::Pipeline,
};

impl SeriesPipeline<Page, Episode> for Pipeline {
    async fn get_episode_queue(&self, url: Url) -> Result<Vec<EpisodeQueueItem>> {
        let series_id = self.client.get_series_id(url).await?;
        let feed = self.client.get_series_feed(&series_id).await?;
        let mut entries = feed.entries();
        entries.sort_by_key(|entry| entry.updated());

        entries
            .iter()
            .map(|entry| {
                let title = entry
                    .title()
                    .context("episode title not found in series feed")?;
                let url = entry
                    .links()
                    .into_iter()
                    .find(|link| link.rel().as_deref() != Some("enclosure"))
                    .context("episode link not found in series feed")?;
                Ok(EpisodeQueueItem::new(&title, Url::parse(&url.url())?))
            })
            .collect()
    }

    async fn download_episode<T: AsRef<Path>>(
        &self,
        url: &Url,
        path: &T,
    ) -> Result<(), PipelineError> {
        let episode = self.client.get_episode_at(url.clone()).await?;
        let title = episode.title().ok_or(PipelineError::Unknown)?;
        let writer = self.file_writer(path).map_err(|_| PipelineError::IoError)?;
        self.download_pages(episode.pages(), writer, &title)
            .await
            .map_err(|_| PipelineError::DownloadError)
    }

    async fn download_series<T: AsRef<Path>>(&self, url: &Url, directory: &T) -> Result<()> {
        let queue = self.get_episode_queue(url.clone()).await?;

        stream::iter(queue)
            .map(|item| {
                let path = self
                    .writer_config
                    .output_path(directory, item.title())
                    .map_err(|_| PipelineError::IoError)?;
                Ok::<_, PipelineError>((item, path))
            })
            .map_ok(|(item, path)| async move { self.download_episode(item.url(), &path).await })
            .try_buffer_unordered(self.num_connections)
            .try_collect::<Vec<_>>()
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "accesses a live manga website"]
    async fn reads_live_series_feed() -> Result<()> {
        let pipeline = Pipeline::default();
        let url = Url::parse("https://shonenjumpplus.com/episode/3270375685341574016")?;
        assert!(!pipeline.get_episode_queue(url).await?.is_empty());
        Ok(())
    }
}
