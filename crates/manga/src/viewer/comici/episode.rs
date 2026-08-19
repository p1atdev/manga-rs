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
        self.client
            .get_image(page)
            .await?
            .bytes()
            .await
            .map(Into::into)
            .map_err(|_| ClientError::DecodeError)
    }

    async fn solve_image_bytes(&self, image: Bytes, page: Option<Page>) -> Result<Bytes> {
        let page = page.context("page is required to solve a Comici image")?;
        tokio::task::spawn_blocking(move || Solver::new(page.scramble().clone()).solve(image))
            .await?
    }

    async fn solve_image(&self, image: Bytes, page: Option<Page>) -> Result<DynamicImage> {
        let page = page.context("page is required to solve a Comici image")?;
        tokio::task::spawn_blocking(move || {
            Solver::new(page.scramble().clone()).solve_from_bytes(image)
        })
        .await?
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
        let episode = self.client.get_episode_at(url.clone()).await?;
        let title = episode.title().context("episode title not found")?;
        let path =
            self.writer_config
                .episode_output_path(directory, episode.series_title(), &title)?;
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
        let url = Url::parse("https://bibibi-comic.com/episodes/7e06f5b186c99")?;
        Pipeline::for_base_url(Url::parse("https://bibibi-comic.com")?)
            .download(&url, &"tests/output/live-comici")
            .await
    }
}
