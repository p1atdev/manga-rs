use std::{future::Future, path::Path};

use anyhow::Result;
use image::DynamicImage;
use url::Url;

use crate::{
    data::{MangaEpisode, MangaPage},
    progress::ProgressConfig,
    utils::Bytes,
};

/// How to save the manga
#[derive(Debug, Clone)]
pub enum SaveFormat {
    Raw,
    Zip {
        compression_method: zip::CompressionMethod,
    },
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
    fn website(self, website: W) -> Self;
    fn progress(self, progress: ProgressConfig) -> Self;
    fn writer_config(self, writer_config: WriterConifg) -> Self;
    fn num_threads(self, num_threads: usize) -> Self;
    fn num_connections(self, num_connections: usize) -> Self;
}

/// Pipeline to download manga
pub trait EpisodePipeline<P: MangaPage, E: MangaEpisode<P>> {
    fn parse_episode_id(&self, url: &Url) -> Result<String>;

    /// Fetch the Episode
    fn fetch_episode(&self, episode_id: &str) -> impl Future<Output = Result<E>> + Send;

    /// Fetch an image
    fn fetch_image(&self, page: &P) -> impl Future<Output = Result<Bytes>> + Send;

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

    fn write_image_bytes<T: AsRef<Path>>(
        &self,
        images: Vec<Bytes>,
        path: T,
    ) -> impl Future<Output = Result<()>>;

    fn write_images<T: AsRef<Path>>(
        &self,
        images: Vec<DynamicImage>,
        path: T,
    ) -> impl Future<Output = Result<()>>;

    fn download<T: AsRef<Path>>(&self, url: &Url, path: T) -> impl Future<Output = Result<()>>;
}
