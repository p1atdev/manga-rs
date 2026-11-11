use super::{Client, GuestViewer, ViewerPage, episode_ids, solve_page};
use crate::{
    data::{IndexNumber, MangaEpisode, MangaPage},
    error::ClientError,
    io::FileWriter,
    pipeline::{DownloadLimits, EpisodePipeline, EpisodePipelineBuilder, SaveFormat, WriterConfig},
    progress::ProgressConfig,
    utils::{Bytes, encode_image},
};
use anyhow::{Context, Result};
use image::DynamicImage;
use std::path::Path;
use url::Url;

pub struct Page {
    index: usize,
    page: ViewerPage,
    scrambled: bool,
}
impl MangaPage for Page {
    fn index(&self) -> Result<usize> {
        Ok(self.index)
    }
    fn is_image(&self) -> bool {
        true
    }
}
pub struct Episode {
    id: String,
    title: String,
    pages: Vec<Page>,
}
impl Episode {
    pub fn from_viewer(id: String, title: String, viewer: GuestViewer) -> Self {
        Self {
            id,
            title,
            pages: viewer
                .pages
                .into_iter()
                .enumerate()
                .map(|(index, page)| Page {
                    index,
                    page,
                    scrambled: viewer.scrambled,
                })
                .collect(),
        }
    }
}
impl MangaEpisode<Page> for Episode {
    fn id(&self) -> String {
        self.id.clone()
    }
    // The listing does not provide a reliable ordinal; use the episode ID.
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

/// Piccoma pipeline for explicitly guest-free episodes.
#[derive(Clone)]
pub struct Pipeline {
    client: Client,
    progress: ProgressConfig,
    writer_config: WriterConfig,
    limits: DownloadLimits,
}
impl Pipeline {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::new()?,
            progress: ProgressConfig::default(),
            writer_config: WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png),
            limits: DownloadLimits::new(num_cpus::get(), 8),
        })
    }
    async fn episode_at(&self, url: &Url) -> Result<Episode> {
        let (product, id) = episode_ids(url)?;
        let (title, viewer) = self.client.get_episode(url).await?;
        Ok(Episode::from_viewer(
            format!("{product}/{id}"),
            title,
            viewer,
        ))
    }
}
impl EpisodePipelineBuilder for Pipeline {
    fn set_progress(mut self, value: ProgressConfig) -> Self {
        self.progress = value;
        self
    }
    fn set_writer_config(mut self, value: WriterConfig) -> Self {
        self.writer_config = value;
        self
    }
    fn set_num_threads(mut self, value: usize) -> Self {
        self.limits.set_num_threads(value);
        self
    }
    fn set_num_connections(mut self, value: usize) -> Self {
        self.limits.set_num_connections(value);
        self
    }
}
impl EpisodePipeline<Page, Episode> for Pipeline {
    async fn fetch_episode(&self, id: &str) -> Result<Episode, ClientError> {
        let (product, episode) = id.split_once('/').ok_or(ClientError::InvalidUrl)?;
        ensure_id(product)?;
        ensure_id(episode)?;
        let url = Url::parse(&format!(
            "https://piccoma.com/web/viewer/{product}/{episode}"
        ))
        .map_err(|_| ClientError::InvalidUrl)?;
        self.episode_at(&url)
            .await
            .map_err(|_| ClientError::RequestError)
    }
    async fn fetch_image(&self, page: &Page) -> Result<Bytes, ClientError> {
        self.client
            .fetch_page(&page.page)
            .await
            .map_err(|_| ClientError::RequestError)
    }
    async fn solve_image_bytes(&self, bytes: Bytes, page: Option<Page>) -> Result<Bytes> {
        let image = self.solve_image(bytes, page).await?;
        encode_image(&image, image::ImageFormat::Png)
    }
    async fn solve_image(&self, bytes: Bytes, page: Option<Page>) -> Result<DynamicImage> {
        let page = page.context("page is required to solve a Piccoma image")?;
        tokio::task::spawn_blocking(move || solve_page(&bytes, &page.page, page.scrambled)).await?
    }
    fn file_writer<P: AsRef<Path>>(&self, path: &P) -> Result<FileWriter> {
        FileWriter::new(&self.writer_config, path)
    }
    fn progress(&self) -> &ProgressConfig {
        &self.progress
    }
    fn download_limits(&self) -> &DownloadLimits {
        &self.limits
    }
    async fn download<P: AsRef<Path>>(&self, url: &Url, path: &P) -> Result<()> {
        let episode = self.episode_at(url).await?;
        let title = episode.title.clone();
        self.download_pages(episode.into_pages(), self.file_writer(path)?, &title)
            .await
    }
    async fn download_in<P: AsRef<Path>>(&self, url: &Url, directory: &P) -> Result<()> {
        let episode = self.episode_at(url).await?;
        let title = episode.title.clone();
        let path = self.writer_config.output_path(directory, &title)?;
        self.download_pages(episode.into_pages(), self.file_writer(&path)?, &title)
            .await
    }
}
fn ensure_id(id: &str) -> Result<(), ClientError> {
    if id.is_empty() || id.len() > 20 || !id.bytes().all(|c| c.is_ascii_digit()) {
        return Err(ClientError::InvalidUrl);
    }
    Ok(())
}
