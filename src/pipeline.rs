use std::{future::Future, path::Path};

use anyhow::{bail, Context, Result};
use image::DynamicImage;
use url::Url;

use crate::{
    data::{MangaEpisode, MangaPage},
    progress::ProgressConfig,
    utils::Bytes,
    viewer::{fuz, giga, ViewerType, ViewerWebsite},
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

/// Piepline for downloading manga
pub trait EpisodePipelineBuilder<W, A: MangaPage, B: MangaEpisode<A>, P: EpisodePipeline<A, B>> {
    fn default() -> Self;
    fn website(self, website: W) -> Self;
    fn progress(self, progress: ProgressConfig) -> Self;
    fn writer_config(self, writer_config: WriterConifg) -> Self;
    fn num_threads(self, num_threads: usize) -> Self;
    fn build(&self) -> P;
}

pub trait EpisodePipeline<P: MangaPage, E: MangaEpisode<P>> {
    fn parse_episode_id(&self, url: &Url) -> Result<String>;
    fn fetch_episode(&self, episode_id: &str) -> impl Future<Output = Result<E>> + Send;
    fn fetch_pages(&self, episode: E) -> impl Future<Output = Result<Vec<Bytes>>> + Send;
    fn solve_images(
        &self,
        images: Vec<Bytes>,
    ) -> impl Future<Output = Result<Vec<DynamicImage>>> + Send;
    fn write_images<T: AsRef<Path>>(
        &self,
        images: Vec<DynamicImage>,
        path: T,
    ) -> impl Future<Output = Result<()>>;
    fn download<T: AsRef<Path>>(&self, url: &Url, path: T) -> impl Future<Output = Result<()>>;
}
