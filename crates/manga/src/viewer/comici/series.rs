use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
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
        let (series_id, series_title) = self.client.get_series_metadata_at(url).await?;
        self.client
            .get_accessible_series_episodes(&series_id)
            .await?
            .into_iter()
            .map(|episode| {
                Ok(EpisodeQueueItem::new(
                    &series_title,
                    episode.title(),
                    self.client.episode_url(episode.id())?,
                ))
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
                    let result = match self.writer_config.episode_output_path(
                        directory,
                        item.series_title(),
                        item.title(),
                    ) {
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
    async fn reads_live_series_queue() -> Result<()> {
        let pipeline = Pipeline::for_base_url(Url::parse("https://youngchampion.jp")?);
        let url = Url::parse("https://youngchampion.jp/episodes/91ae3b5263e18")?;
        assert!(!pipeline.get_episode_queue(url).await?.is_empty());
        Ok(())
    }
}
