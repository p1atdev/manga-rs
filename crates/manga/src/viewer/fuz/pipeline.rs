use std::path::Path;

use anyhow::{Context, Result, bail};
use image::DynamicImage;
use url::Url;

use crate::{
    data::{MangaEpisode, MangaPage},
    error::ClientError,
    io::FileWriter,
    pipeline::{EpisodePipeline, EpisodePipelineBuilder, SaveFormat, WriterConfig},
    progress::ProgressConfig,
    solver::ImageSolver,
    utils::Bytes,
    viewer::{ViewerClient, ViewerConfigBuilder},
};

use super::{
    data::{Episode, Page},
    solver::Solver,
    viewer::{Client, ConfigBuilder},
};

#[derive(Debug, Clone)]
pub struct Pipeline {
    client: Client,
    progress: ProgressConfig,
    writer_config: WriterConfig,
    num_threads: usize,
    num_connections: usize,
}

impl Pipeline {
    pub fn new(
        base_url: Url,
        api_url: Url,
        image_url: Url,
        progress: ProgressConfig,
        writer_config: WriterConfig,
        num_threads: usize,
        num_connections: usize,
    ) -> Self {
        Self {
            client: Client::new(ConfigBuilder::new(base_url, api_url, image_url).build()),
            progress,
            writer_config,
            num_threads,
            num_connections,
        }
    }

    pub fn for_urls(base_url: Url, api_url: Url, image_url: Url) -> Self {
        Self {
            client: Client::new(ConfigBuilder::new(base_url, api_url, image_url).build()),
            progress: ProgressConfig::default(),
            writer_config: WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png),
            num_threads: num_cpus::get(),
            num_connections: 8,
        }
    }
}

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

    fn set_num_threads(self, num_threads: usize) -> Self {
        Self {
            num_threads,
            ..self
        }
    }

    fn set_num_connections(self, num_connections: usize) -> Self {
        Self {
            num_connections,
            ..self
        }
    }
}

impl EpisodePipeline<Page, Episode> for Pipeline {
    async fn fetch_episode(&self, episode_id: &str) -> Result<Episode, ClientError> {
        self.client.get_episode(episode_id).await
    }

    async fn fetch_image(&self, page: &Page) -> Result<Bytes, ClientError> {
        let path = page.image_path().map_err(|_| ClientError::InvalidPage)?;
        let url = self
            .client
            .image_url(path)
            .map_err(|_| ClientError::InvalidUrl)?;
        self.client
            .get(url)
            .await?
            .bytes()
            .await
            .map(Into::into)
            .map_err(|_| ClientError::DecodeError)
    }

    async fn solve_image_bytes(&self, bytes: Bytes, page: Option<Page>) -> Result<Bytes> {
        match page.context("page is required to solve a FUZ image")? {
            Page::Image(page) => {
                Solver::new(page.encryption_key(), page.encryption_iv()).solve(bytes)
            }
            _ => bail!("page is not an image"),
        }
    }

    async fn solve_image(&self, bytes: Bytes, page: Option<Page>) -> Result<DynamicImage> {
        match page.context("page is required to solve a FUZ image")? {
            Page::Image(page) => {
                tokio::task::spawn_blocking(move || {
                    Solver::new(page.encryption_key(), page.encryption_iv()).solve_from_bytes(bytes)
                })
                .await?
            }
            _ => bail!("page is not an image"),
        }
    }

    fn file_writer<P: AsRef<Path>>(&self, path: &P) -> Result<FileWriter> {
        FileWriter::new(&self.writer_config, path)
    }

    fn progress(&self) -> &ProgressConfig {
        &self.progress
    }

    fn num_threads(&self) -> usize {
        self.num_threads
    }

    fn num_connections(&self) -> usize {
        self.num_connections
    }

    async fn download<P: AsRef<Path>>(&self, url: &Url, path: &P) -> Result<()> {
        let episode_id = self
            .client
            .parse_episode_id(url)
            .context("failed to parse FUZ episode id")?;
        let episode = self.fetch_episode(&episode_id).await?;
        let title = episode.title().context("episode title not found")?;
        let pages = episode
            .pages()
            .into_iter()
            .filter(MangaPage::is_image)
            .collect();
        self.download_pages(pages, self.file_writer(path)?, &title)
            .await
    }

    async fn download_in<P: AsRef<Path>>(&self, url: &Url, directory: &P) -> Result<()> {
        let episode_id = self
            .client
            .parse_episode_id(url)
            .context("failed to parse FUZ episode id")?;
        let episode = self.fetch_episode(&episode_id).await?;
        let title = episode.title().context("episode title not found")?;
        let pages = episode
            .pages()
            .into_iter()
            .filter(MangaPage::is_image)
            .collect();
        let path = self.writer_config.output_path(directory, &title)?;
        self.download_pages(pages, self.file_writer(&path)?, &title)
            .await
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "FUZ protobuf API is currently known to be unstable"]
    async fn downloads_live_episode() -> Result<()> {
        let url = Url::parse("https://comic-fuz.com/manga/viewer/44994")?;
        Pipeline::for_urls(
            Url::parse("https://comic-fuz.com")?,
            Url::parse("https://api.comic-fuz.com")?,
            Url::parse("https://img.comic-fuz.com")?,
        )
        .download(&url, &"tests/output/live-fuz")
        .await
    }
}
