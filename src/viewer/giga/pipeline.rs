use std::{path::Path, sync::Arc};

use anyhow::{Context, Ok, Result};
use futures::StreamExt;
use image::DynamicImage;
use indicatif::ParallelProgressIterator;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use url::Url;

use crate::{
    data::{MangaEpisode, MangaPage},
    io::{pdf::PdfWriter, raw::RawWriter, zip::ZipWriter, EpisodeWriter},
    pipeline::{EpisodePipeline, EpisodePipelineBuilder, SaveFormat, WriterConifg},
    progress::ProgressConfig,
    solver::ImageSolver,
    utils::Bytes,
    viewer::{ViewerClient, ViewerConfigBuilder},
};

use super::{
    data::{Episode, Page},
    solver::Solver,
    viewer::{Client, ConfigBuilder, Website},
};

pub struct PipelineBuilder {
    website: Website,
    progress: ProgressConfig,
    writer_config: WriterConifg,
    num_threads: usize,
}

impl EpisodePipelineBuilder<Website, Page, Episode, Pipeline> for PipelineBuilder {
    fn default() -> Self {
        Self {
            website: Website::ShonenJumpPlus,
            progress: ProgressConfig::default(),
            writer_config: WriterConifg::new(SaveFormat::Raw, image::ImageFormat::Png),
            num_threads: num_cpus::get(),
        }
    }

    fn website(self, website: Website) -> Self {
        Self { website, ..self }
    }

    fn progress(self, progress: ProgressConfig) -> Self {
        Self { progress, ..self }
    }

    fn writer_config(self, writer_config: WriterConifg) -> Self {
        Self {
            writer_config,
            ..self
        }
    }

    fn num_threads(self, num_threads: usize) -> Self {
        Self {
            num_threads,
            ..self
        }
    }

    fn build(&self) -> Pipeline {
        Pipeline::new(
            self.website.clone(),
            self.progress.clone(),
            self.writer_config.clone(),
            self.num_threads,
        )
    }
}

/// Pipeline for downloading an episode of ChojuGiga manga
pub struct Pipeline {
    client: Client,
    progress: ProgressConfig,
    writer_config: WriterConifg,
    num_threads: usize,
}

impl Pipeline {
    pub fn new(
        website: Website,
        progress: ProgressConfig,
        writer_config: WriterConifg,
        num_threads: usize,
    ) -> Self {
        let client = Client::new(ConfigBuilder::new(website).build());
        Self {
            client,
            progress,
            writer_config,
            num_threads,
        }
    }
}

impl EpisodePipeline<Page, Episode> for Pipeline {
    fn parse_episode_id(&self, url: &Url) -> Result<String> {
        self.client
            .parse_episode_id(url)
            .context("Failed to parse episode id")
    }

    async fn fetch_episode(&self, episode_id: &str) -> Result<Episode> {
        self.client.get_episode(episode_id).await
    }

    async fn fetch_pages(&self, episode: Episode) -> Result<Vec<Vec<u8>>> {
        let pages = episode.pages();
        let pages_len = pages.len();

        let pages = self
            .progress
            .build(pages_len)?
            .wrap_stream(futures::stream::iter(pages))
            .map(|page| {
                let client = self.client.clone();

                tokio::spawn(async move {
                    let url = page.url()?;
                    let res = client.get(url).await?;
                    let bytes = res.bytes().await?;

                    Result::<_>::Ok(bytes.into())
                })
            })
            .buffer_unordered(4)
            .map(|bytes| bytes?)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        Ok(pages)
    }

    async fn solve_images(&self, images: Vec<Bytes>) -> Result<Vec<DynamicImage>> {
        let solver = Arc::new(Solver::new());
        let images = images
            .par_iter()
            .progress_with(self.progress.build(images.len())?)
            .map(|bytes| {
                let image = solver.solve_from_bytes(bytes)?;
                Result::<_>::Ok(image)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(images)
    }

    async fn write_images<T: AsRef<Path>>(&self, images: Vec<DynamicImage>, path: T) -> Result<()> {
        let writer_config = &self.writer_config;

        match writer_config.save_format() {
            SaveFormat::Raw => {
                let writer = RawWriter::new(
                    self.progress.clone(),
                    self.writer_config.image_format(),
                    self.num_threads,
                );
                writer.write(images, path).await?;
            }
            SaveFormat::Zip { compression_method } => {
                let writer = ZipWriter::new(
                    compression_method,
                    self.writer_config.image_format(),
                    self.num_threads,
                    self.progress.clone(),
                );
                writer.write(images, path).await?;
            }
            SaveFormat::Pdf => {
                let writer =
                    PdfWriter::new(self.progress.clone(), self.writer_config.image_format());
                writer.write(images, path).await?;
            }
        }

        Ok(())
    }

    async fn download<T: AsRef<Path>>(&self, url: &Url, path: T) -> Result<()> {
        let episode_id = self.parse_episode_id(url)?;
        let episode = self.fetch_episode(&episode_id).await?;
        let pages = self.fetch_pages(episode).await?;
        let images = self.solve_images(pages).await?;
        self.write_images(images, path).await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[tokio::test]
    async fn test_pipeline_download_raw() -> Result<()> {
        let url = Url::parse("https://shonenjumpplus.com/episode/16457717013869519536")?;
        let path = "playground/output/giga_pipe_raw";

        let builder = PipelineBuilder::default();
        let pipe = builder.build();

        pipe.download(&url, path).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_pipeline_download_zip() -> Result<()> {
        let url = Url::parse("https://shonenjumpplus.com/episode/16457717013869519536")?;
        let path = "playground/output/giga_pipe_zip.zip";

        let builder = PipelineBuilder::default().writer_config(WriterConifg::new(
            SaveFormat::Zip {
                compression_method: zip::CompressionMethod::Zstd,
            },
            image::ImageFormat::WebP,
        ));
        let pipe = builder.build();

        pipe.download(&url, path).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_pipeline_download_pdf() -> Result<()> {
        let url = Url::parse("https://shonenjumpplus.com/episode/16457717013869519536")?;
        let path = "playground/output/giga_pipe_pdf.pdf";

        let builder = PipelineBuilder::default()
            .writer_config(WriterConifg::new(SaveFormat::Pdf, image::ImageFormat::Jpeg));
        let pipe = builder.build();

        pipe.download(&url, path).await?;
        Ok(())
    }
}
