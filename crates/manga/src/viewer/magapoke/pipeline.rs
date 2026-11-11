use super::{Client, Viewer, decode_image, episode_ids, restore_image};
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
    url: String,
    title: u32,
    episode: u32,
    seed: String,
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
    title: u32,
    episode: u32,
    pages: Vec<Page>,
}
impl Episode {
    pub fn from_viewer(viewer: Viewer) -> Self {
        let title = viewer.title_id;
        let episode = viewer.episode_id;
        let pages = viewer
            .page_list
            .into_iter()
            .enumerate()
            .map(|(index, url)| Page {
                index,
                url,
                title,
                episode,
                seed: viewer.scramble_seed.clone(),
            })
            .collect();
        Self {
            title,
            episode,
            pages,
        }
    }
}
impl MangaEpisode<Page> for Episode {
    fn id(&self) -> String {
        format!("{}/{}", self.title, self.episode)
    }
    // Guest viewer metadata does not provide an ordinal; use a unique ID.
    fn index(&self) -> IndexNumber {
        IndexNumber::String(self.id())
    }
    fn title(&self) -> Option<String> {
        Some(self.episode.to_string())
    }
    fn pages(&self) -> &[Page] {
        &self.pages
    }
    fn into_pages(self) -> Vec<Page> {
        self.pages
    }
}
/// Guest-free Magazine Pocket pipeline.
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
        episode_ids(url)?;
        let viewer = self.client.get_episode(url).await?;
        Ok(Episode::from_viewer(viewer))
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
        let (title, episode) = id.split_once('/').ok_or(ClientError::InvalidUrl)?;
        let title: u32 = title.parse().map_err(|_| ClientError::InvalidUrl)?;
        let episode: u32 = episode.parse().map_err(|_| ClientError::InvalidUrl)?;
        if title == 0
            || title > 99_999
            || episode == 0
            || episode > 999_999_999
            || id != format!("{title}/{episode}")
        {
            return Err(ClientError::InvalidUrl);
        }
        let url = Url::parse(&format!(
            "https://pocket.shonenmagazine.com/title/{title:05}/episode/{episode}"
        ))
        .map_err(|_| ClientError::InvalidUrl)?;
        self.episode_at(&url)
            .await
            .map_err(|_| ClientError::RequestError)
    }
    async fn fetch_image(&self, page: &Page) -> Result<Bytes, ClientError> {
        self.client
            .fetch_page(&page.url)
            .await
            .map_err(|_| ClientError::RequestError)
    }
    async fn solve_image_bytes(&self, bytes: Bytes, page: Option<Page>) -> Result<Bytes> {
        let image = self.solve_image(bytes, page).await?;
        encode_image(&image, image::ImageFormat::Png)
    }
    async fn solve_image(&self, bytes: Bytes, page: Option<Page>) -> Result<DynamicImage> {
        let page = page.context("page is required to solve a Magazine Pocket image")?;
        tokio::task::spawn_blocking(move || {
            Ok(DynamicImage::ImageRgba8(restore_image(
                decode_image(&bytes)?,
                page.title,
                page.episode,
                &page.seed,
            )?))
        })
        .await?
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
        let title = episode.title().context("episode ID missing")?;
        self.download_pages(episode.into_pages(), self.file_writer(path)?, &title)
            .await
    }
    async fn download_in<P: AsRef<Path>>(&self, url: &Url, directory: &P) -> Result<()> {
        let episode = self.episode_at(url).await?;
        let title = episode.title().context("episode ID missing")?;
        let path = self.writer_config.output_path(directory, &title)?;
        self.download_pages(episode.into_pages(), self.file_writer(&path)?, &title)
            .await
    }
}
