use super::{ViewerPage, anonymous_client, episode_url, fetch_viewer, reader_get, solve_page};
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

impl MangaPage for ViewerPage {
    fn index(&self) -> Result<usize> {
        Ok(self.index)
    }
    fn is_image(&self) -> bool {
        true
    }
}
pub struct Episode {
    url: Url,
    title: String,
    pages: Vec<ViewerPage>,
}
impl MangaEpisode<ViewerPage> for Episode {
    fn id(&self) -> String {
        self.url.to_string()
    }
    // The viewer does not provide a reliable ordinal; use the episode URL.
    fn index(&self) -> IndexNumber {
        IndexNumber::String(self.url.to_string())
    }
    fn title(&self) -> Option<String> {
        Some(self.title.clone())
    }
    fn pages(&self) -> &[ViewerPage] {
        &self.pages
    }
    fn into_pages(self) -> Vec<ViewerPage> {
        self.pages
    }
}

/// Anonymous YanMaga pipeline. Only full viewer access is accepted.
/// A caller cannot provide a client with automatic redirects enabled:
/// ```compile_fail
/// use manga::viewer::yanmaga::fetch_viewer;
/// ```
#[derive(Clone)]
pub struct Pipeline {
    client: reqwest::Client,
    progress: ProgressConfig,
    writer_config: WriterConfig,
    limits: DownloadLimits,
}
impl Pipeline {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: anonymous_client()?,
            progress: ProgressConfig::default(),
            writer_config: WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png),
            limits: DownloadLimits::new(num_cpus::get(), 8),
        })
    }
    async fn episode_at(&self, url: &Url) -> Result<Episode> {
        episode_url(url)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let entropy: String = format!("{stamp:032x}").chars().rev().collect();
        let pages = fetch_viewer(&self.client, url.as_str(), &entropy).await?;
        let title = url
            .path_segments()
            .and_then(|mut p| p.next_back())
            .context("episode ID missing")?
            .to_owned();
        Ok(Episode {
            url: url.clone(),
            title,
            pages,
        })
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
impl EpisodePipeline<ViewerPage, Episode> for Pipeline {
    /// The episode ID is its complete official URL.
    async fn fetch_episode(&self, id: &str) -> Result<Episode, ClientError> {
        let url = Url::parse(id).map_err(|_| ClientError::InvalidUrl)?;
        self.episode_at(&url)
            .await
            .map_err(|_| ClientError::RequestError)
    }
    async fn fetch_image(&self, page: &ViewerPage) -> Result<Bytes, ClientError> {
        reader_get(&self.client, page.url.clone(), 20_000_000)
            .await
            .map_err(|_| ClientError::RequestError)
    }
    async fn solve_image_bytes(&self, bytes: Bytes, page: Option<ViewerPage>) -> Result<Bytes> {
        let image = self.solve_image(bytes, page).await?;
        encode_image(&image, image::ImageFormat::Png)
    }
    async fn solve_image(&self, bytes: Bytes, page: Option<ViewerPage>) -> Result<DynamicImage> {
        let page = page.context("page is required to solve a YanMaga image")?;
        tokio::task::spawn_blocking(move || solve_page(&bytes, &page)).await?
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn episode_index_preserves_unique_url_instead_of_claiming_order() {
        let url =
            Url::parse("https://yanmaga.jp/comics/test/0123456789abcdef0123456789abcdef").unwrap();
        let episode = Episode {
            url: url.clone(),
            title: "test".into(),
            pages: vec![],
        };
        assert_eq!(episode.index(), IndexNumber::String(url.to_string()));
    }
}
