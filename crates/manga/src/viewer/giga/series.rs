use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result};
use futures::{StreamExt, stream};
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
        self.download_limits
            .validate()
            .map_err(|_| PipelineError::Unknown)?;
        let episode = {
            let connection_permit = self
                .download_limits
                .acquire_connection()
                .await
                .map_err(|_| PipelineError::Unknown)?;
            let episode = self.client.get_episode_at(url.clone()).await?;
            drop(connection_permit);
            episode
        };
        let title = episode.title().ok_or(PipelineError::Unknown)?;
        let writer = self.file_writer(path).map_err(|_| PipelineError::IoError)?;
        self.download_pages(episode.into_pages(), writer, &title)
            .await
            .map_err(|_| PipelineError::DownloadError)
    }

    async fn download_series<T: AsRef<Path>>(&self, url: &Url, directory: &T) -> Result<()> {
        let queue = self.get_episode_queue(url.clone()).await?;
        self.download_limits.validate()?;
        let cancelled = AtomicBool::new(false);

        let processing_error = stream::iter(queue)
            .map(|item| {
                let cancelled = &cancelled;
                async move {
                    if cancelled.load(Ordering::Acquire) {
                        return None;
                    }
                    let result = match self.writer_config.output_path(directory, item.title()) {
                        Ok(path) => self.download_episode(item.url(), &path).await,
                        Err(_) => Err(PipelineError::IoError),
                    };
                    if result.is_err() {
                        cancelled.store(true, Ordering::Release);
                    }
                    Some(result)
                }
            })
            .buffer_unordered(self.download_limits.num_connections())
            .fold(None, |first_error, result| {
                let next_error = match result {
                    Some(Err(error)) => first_error.or(Some(error)),
                    Some(Ok(())) | None => first_error,
                };
                async move { next_error }
            })
            .await;

        if let Some(error) = processing_error {
            return Err(error.into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "accesses a live manga website"]
    async fn reads_live_series_feed() -> Result<()> {
        let pipeline = Pipeline::for_base_url(Url::parse("https://shonenjumpplus.com")?);
        let url = Url::parse("https://shonenjumpplus.com/episode/3270375685341574016")?;
        assert!(!pipeline.get_episode_queue(url).await?.is_empty());
        Ok(())
    }
}
