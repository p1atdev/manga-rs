use std::{path::Path, sync::Arc};

use anyhow::{Context, Ok, Result};
use futures::{stream, StreamExt};
use image::DynamicImage;
use indicatif::ParallelProgressIterator;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator,
};
use url::Url;

use crate::{
    data::{MangaEpisode, MangaPage},
    io::{pdf::PdfWriter, raw::RawWriter, zip::ZipWriter, EpisodeWriter},
    pipeline::{EpisodePipeline, EpisodePipelineBuilder, SaveFormat, WriterConifg},
    progress::ProgressConfig,
    solver::ImageSolver,
    utils::{self, Bytes},
    viewer::{ViewerClient, ViewerConfigBuilder},
};

use super::{
    data::{Episode, Page},
    solver::Solver,
    viewer::{Client, ConfigBuilder, Website},
};

#[derive(Debug, Clone)]
pub struct PipelineBuilder {
    website: Website,
    progress: ProgressConfig,
    writer_config: WriterConifg,
    num_threads: usize,
    num_connections: usize,
}

impl EpisodePipelineBuilder<Website, Page, Episode, Pipeline> for PipelineBuilder {
    fn default() -> Self {
        Self {
            website: Website::ShonenJumpPlus,
            progress: ProgressConfig::default(),
            writer_config: WriterConifg::new(SaveFormat::Raw, image::ImageFormat::Png),
            num_threads: num_cpus::get(),
            num_connections: 8,
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

    fn num_connections(self, num_connections: usize) -> Self {
        Self {
            num_connections,
            ..self
        }
    }

    fn build(&self) -> Pipeline {
        Pipeline::new(
            self.website.clone(),
            self.progress.clone(),
            self.writer_config.clone(),
            self.num_threads,
            self.num_connections,
        )
    }
}

/// Pipeline for downloading an episode of ChojuGiga manga
#[derive(Debug, Clone)]
pub struct Pipeline {
    client: Client,
    progress: ProgressConfig,
    writer_config: WriterConifg,
    num_threads: usize,
    num_connections: usize,
}

impl Pipeline {
    pub fn new(
        website: Website,
        progress: ProgressConfig,
        writer_config: WriterConifg,
        num_threads: usize,
        num_connections: usize,
    ) -> Self {
        let client = Client::new(ConfigBuilder::new(website).build());
        Self {
            client,
            progress,
            writer_config,
            num_threads,
            num_connections,
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

    async fn fetch_images(&self, pages: Vec<Page>) -> Result<Vec<Vec<u8>>> {
        let pages_len = pages.len();

        let mut pages = self
            .progress
            .build_with_message(pages_len, "Fetching images...")?
            .wrap_stream(futures::stream::iter(pages))
            .map(|page| {
                let client = self.client.clone();

                tokio::spawn(async move {
                    let url = page.url()?;
                    let res = client.get(url).await?;
                    let bytes = res.bytes().await?;

                    Result::<_>::Ok((bytes, page.index()?))
                })
            })
            .buffer_unordered(self.num_connections)
            .map(|pair| pair?)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        pages.sort_by_key(|(_, index)| *index);
        let pages = pages
            .into_iter()
            .map(|(bytes, _)| bytes.into())
            .collect::<Vec<_>>();

        Ok(pages)
    }

    async fn solve_image_bytes(
        &self,
        images: Vec<Bytes>,
        _pages: Option<Vec<Page>>, // does not use
    ) -> Result<Vec<Bytes>> {
        let solver = Arc::new(Solver::new());
        let mut images = images
            .par_iter()
            .progress_with(
                self.progress
                    .build_with_message(images.len(), "Solving the image obfuscations...")?,
            )
            .enumerate()
            .map(|(i, bytes)| {
                let image = solver.solve(bytes)?;
                Result::<_>::Ok((image, i))
            })
            .collect::<Result<Vec<_>>>()?;
        images.sort_by_key(|(_, index)| *index);
        let images = images
            .into_iter()
            .map(|(bytes, _)| bytes)
            .collect::<Vec<_>>();

        Ok(images)
    }

    async fn solve_images(
        &self,
        images: Vec<Bytes>,
        _pages: Option<Vec<Page>>,
    ) -> Result<Vec<image::DynamicImage>> {
        let solver = Arc::new(Solver::new());
        let mut images = images
            .par_iter()
            .progress_with(
                self.progress
                    .build_with_message(images.len(), "Solving the image obfuscations...")?,
            )
            .enumerate()
            .map(|(i, bytes)| {
                let image = solver.solve_from_bytes(bytes)?;
                Result::<_>::Ok((image, i))
            })
            .collect::<Result<Vec<_>>>()?;
        images.sort_by_key(|(_, index)| *index);
        let images = images
            .into_iter()
            .map(|(image, _)| image)
            .collect::<Vec<_>>();

        Ok(images)
    }

    async fn write_image_bytes<T: AsRef<Path>>(&self, images: Vec<Bytes>, path: T) -> Result<()> {
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

    async fn write_images<T: AsRef<Path>>(&self, images: Vec<DynamicImage>, path: T) -> Result<()> {
        let writer_config = &self.writer_config;

        match writer_config.save_format() {
            SaveFormat::Raw => {
                let writer = RawWriter::new(
                    self.progress.clone(),
                    self.writer_config.image_format(),
                    self.num_threads,
                );
                writer.write_images(images, path).await?;
            }
            SaveFormat::Zip { compression_method } => {
                let writer = ZipWriter::new(
                    compression_method,
                    self.writer_config.image_format(),
                    self.num_threads,
                    self.progress.clone(),
                );
                writer.write_images(images, path).await?;
            }
            SaveFormat::Pdf => {
                let writer =
                    PdfWriter::new(self.progress.clone(), self.writer_config.image_format());
                writer.write_images(images, path).await?;
            }
        }

        Ok(())
    }

    async fn download<T: AsRef<Path>>(&self, url: &Url, path: T) -> Result<()> {
        let episode_id = self.parse_episode_id(url)?;
        let episode = self.fetch_episode(&episode_id).await?;
        let images = self.fetch_images(episode.pages()).await?;
        let images = self.solve_images(images, None).await?;
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
