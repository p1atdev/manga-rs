use std::{future::Future, path::Path};

use anyhow::Result;
use image::DynamicImage;
use url::Url;

use crate::{
    data::{MangaEpisode, MangaPage},
    error::ClientError,
    io::{EpisodeWriter, FileWriter},
    progress::ProgressConfig,
    utils::Bytes,
};

/// How to save the manga
#[derive(Debug, Clone)]
pub enum SaveFormat {
    Raw,
    Zip {
        compression_method: zip::CompressionMethod,
        extension: Option<String>,
    },
    #[cfg(feature = "pdf")]
    Pdf,
}

/// Configuration for the writer
#[derive(Debug, Clone)]
pub struct WriterConifg {
    save_format: SaveFormat,
    image_format: image::ImageFormat,
}

impl WriterConifg {
    pub fn new(save_format: SaveFormat, image_format: image::ImageFormat) -> Self {
        WriterConifg {
            save_format,
            image_format,
        }
    }

    pub fn save_format(&self) -> SaveFormat {
        self.save_format.clone()
    }

    pub fn image_format(&self) -> image::ImageFormat {
        self.image_format.clone()
    }
}

/// Pipeline configuration trait
pub trait EpisodePipelineBuilder<W, A: MangaPage, B: MangaEpisode<A>, P: EpisodePipeline<A, B>>:
    Default
{
    fn set_website(self, website: W) -> Self;
    fn set_progress(self, progress: ProgressConfig) -> Self;
    fn set_writer_config(self, writer_config: WriterConifg) -> Self;
    fn set_num_threads(self, num_threads: usize) -> Self;
    fn set_num_connections(self, num_connections: usize) -> Self;
}

/// Pipeline to download manga
pub trait EpisodePipeline<P: MangaPage, E: MangaEpisode<P>> {
    fn parse_episode_id(&self, url: &Url) -> Result<String>;

    /// Fetch the Episode
    fn fetch_episode(
        &self,
        episode_id: &str,
    ) -> impl Future<Output = Result<E, ClientError>> + Send;

    /// Fetch an image
    fn fetch_image(&self, page: &P) -> impl Future<Output = Result<Bytes, ClientError>> + Send;

    /// Solve the obfuscation
    fn solve_image_bytes(
        &self,
        image: Bytes,
        page: Option<P>,
    ) -> impl Future<Output = Result<Bytes>> + Send;

    /// Solve the obfuscation and return the image
    fn solve_image(
        &self,
        image: Bytes,
        page: Option<P>,
    ) -> impl Future<Output = Result<DynamicImage>> + Send;

    // fn get_writer<Pa: AsRef<Path>>(&self, path: Pa) -> Result<FileWriter>;

    fn file_writer<T: AsRef<Path>>(&self, save_path: &T) -> Result<FileWriter>;

    async fn write(
        &self,
        file_writer: &FileWriter,
        page: usize,
        image: DynamicImage,
    ) -> Result<()> {
        match file_writer {
            FileWriter::Raw(writer) => {
                writer.write_page(page, image).await?;
            }
            FileWriter::Zip(writer) => {
                writer.write_page(page, image).await?;
            }
            #[cfg(feature = "pdf")]
            FileWriter::Pdf(writer) => {
                writer.write_page(page, image).await?;
            }
        }

        Ok(())
    }

    /// Just download in the specified path
    fn download<T: AsRef<Path>>(&self, url: &Url, path: &T) -> impl Future<Output = Result<()>>;

    /// Download with a new folder or file in the specified directory
    fn download_in<T: AsRef<Path>>(&self, url: &Url, dir: &T) -> impl Future<Output = Result<()>>;
}

#[derive(Clone, Debug)]
pub struct EpisodeQueueItem {
    title: String,
    url: Url,
}

impl EpisodeQueueItem {
    pub fn new(title: &str, url: Url) -> Self {
        EpisodeQueueItem {
            title: title.to_string(),
            url,
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn url(&self) -> &Url {
        &self.url
    }
}

/// Pipeline to download multiple episodes
pub trait SeriesPipeline<P: MangaPage, E: MangaEpisode<P>> {
    fn get_episode_urls(&self, url: Url) -> impl Future<Output = Result<Vec<Url>>>;

    /// Download with a new folder or file in the specified directory
    fn download_series<T: AsRef<Path>>(
        &self,
        url: &Url,
        dir: &T,
    ) -> impl Future<Output = Result<()>>;

    /// Download multiple episodes specified by the urls
    fn download_episodes<T: AsRef<Path>>(
        &self,
        urls: Vec<Url>,
        dir: &T,
    ) -> impl Future<Output = Result<()>>;
}
