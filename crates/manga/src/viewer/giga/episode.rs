use std::path::Path;

use anyhow::{Context, Result};
use image::DynamicImage;
use url::Url;

use crate::{
    data::MangaEpisode,
    error::ClientError,
    io::FileWriter,
    pipeline::{DownloadLimits, EpisodePipeline, EpisodePipelineBuilder, WriterConfig},
    progress::ProgressConfig,
    solver::ImageSolver,
    utils::Bytes,
};

use super::{
    data::{Episode, Page},
    pipeline::Pipeline,
    solver::Solver,
};

impl EpisodePipelineBuilder for Pipeline {
    fn set_progress(self, progress: ProgressConfig) -> Self {
        Self { progress, ..self }
    }

    fn set_writer_config(self, writer_config: WriterConfig) -> Self {
        Self {
            writer_config,
            ..self
        }
    }

    fn set_num_threads(mut self, num_threads: usize) -> Self {
        self.download_limits.set_num_threads(num_threads);
        self
    }

    fn set_num_connections(mut self, num_connections: usize) -> Self {
        self.download_limits.set_num_connections(num_connections);
        self
    }
}

impl EpisodePipeline<Page, Episode> for Pipeline {
    async fn fetch_episode(&self, episode_id: &str) -> Result<Episode, ClientError> {
        self.client.get_episode(episode_id).await
    }

    async fn fetch_image(&self, page: &Page) -> Result<Bytes, ClientError> {
        let response = self
            .client
            .get(page.url().map_err(|_| ClientError::InvalidPage)?)
            .await?;
        response
            .bytes()
            .await
            .map(Into::into)
            .map_err(|_| ClientError::DecodeError)
    }

    async fn solve_image_bytes(&self, image: Bytes, _page: Option<Page>) -> Result<Bytes> {
        tokio::task::spawn_blocking(move || Solver::new().solve(image)).await?
    }

    async fn solve_image(&self, image: Bytes, _page: Option<Page>) -> Result<DynamicImage> {
        tokio::task::spawn_blocking(move || Solver::new().solve_from_bytes(image)).await?
    }

    fn file_writer<P: AsRef<Path>>(&self, save_path: &P) -> Result<FileWriter> {
        FileWriter::new(&self.writer_config, save_path)
    }

    fn progress(&self) -> &ProgressConfig {
        &self.progress
    }

    fn download_limits(&self) -> &DownloadLimits {
        &self.download_limits
    }

    async fn download<P: AsRef<Path>>(&self, url: &Url, path: &P) -> Result<()> {
        let episode = self.client.get_episode_at(url.clone()).await?;
        let title = episode.title().context("episode title not found")?;
        let writer = self.file_writer(path)?;
        self.download_pages(episode.into_pages(), writer, &title)
            .await
    }

    async fn download_in<P: AsRef<Path>>(&self, url: &Url, directory: &P) -> Result<()> {
        let (episode, metadata) = self
            .client
            .get_episode_with_metadata_at(url.clone())
            .await?;
        let title = episode.title().context("episode title not found")?;
        let path =
            self.writer_config
                .episode_output_path(directory, metadata.series_title(), &title)?;
        let writer = self.file_writer(&path)?;
        self.download_pages(episode.into_pages(), writer, &title)
            .await
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "accesses a live manga website"]
    async fn downloads_live_episode() -> Result<()> {
        let url = Url::parse("https://shonenjumpplus.com/episode/16457717013869519536")?;
        Pipeline::for_base_url(Url::parse("https://shonenjumpplus.com")?)
            .download(&url, &"tests/output/live-giga")
            .await
    }
}
