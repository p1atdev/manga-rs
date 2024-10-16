use std::{path::Path, sync::Arc};

use anyhow::{Context, Ok, Result};
use futures::{stream, StreamExt, TryStreamExt};
use image::DynamicImage;
use rayon::slice::ParallelSliceMut;
use url::Url;

#[cfg(feature = "pdf")]
use crate::io::pdf::PdfWriter;
use crate::{
    data::MangaEpisode,
    io::{raw::RawWriter, zip::ZipWriter, EpisodeWriter},
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

/// Pipeline for downloading an episode of ChojuGiga manga
#[derive(Debug, Clone)]
pub struct Pipeline {
    client: Client,
    progress: ProgressConfig,
    writer_config: WriterConifg,
    num_threads: usize,
    num_connections: usize,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self {
            client: Client::new(ConfigBuilder::new(Website::ShonenJumpPlus).build()),
            progress: ProgressConfig::default(),
            writer_config: WriterConifg::new(SaveFormat::Raw, image::ImageFormat::Png),
            num_threads: num_cpus::get(),
            num_connections: 8,
        }
    }
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

impl EpisodePipelineBuilder<Website, Page, Episode, Pipeline> for Pipeline {
    fn set_website(self, website: Website) -> Self {
        let client = Client::new(ConfigBuilder::new(website).build());
        Self { client, ..self }
    }

    fn set_progress(self, progress: ProgressConfig) -> Self {
        Self { progress, ..self }
    }

    fn set_writer_config(self, writer_config: WriterConifg) -> Self {
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
    fn parse_episode_id(&self, url: &Url) -> Result<String> {
        self.client
            .parse_episode_id(url)
            .context("Failed to parse episode id")
    }

    async fn fetch_episode(&self, episode_id: &str) -> Result<Episode> {
        self.client.get_episode(episode_id).await
    }

    async fn fetch_image(&self, page: &Page) -> Result<Bytes> {
        let client = self.client.clone();

        let url = page.url()?;
        let res = client.get(url).await?;
        let bytes = res.bytes().await?;

        Ok(bytes.into())
    }

    async fn solve_image_bytes(&self, image: Bytes, _page: Option<Page>) -> Result<Bytes> {
        let solver = Arc::new(Solver::new());
        let image = solver.solve(image)?;
        Ok(image)
    }

    async fn solve_image(&self, image: Bytes, _page: Option<Page>) -> Result<DynamicImage> {
        let solver = Arc::new(Solver::new());
        let image = solver.solve_from_bytes(image)?;
        Ok(image)
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
            SaveFormat::Zip {
                compression_method,
                extension,
            } => {
                let writer = ZipWriter::new(
                    compression_method,
                    self.writer_config.image_format(),
                    extension,
                    self.num_threads,
                    self.progress.clone(),
                );
                writer.write(images, path).await?;
            }
            #[cfg(feature = "pdf")]
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
            SaveFormat::Zip {
                compression_method,
                extension,
            } => {
                let writer = ZipWriter::new(
                    compression_method,
                    self.writer_config.image_format(),
                    extension,
                    self.num_threads,
                    self.progress.clone(),
                );
                writer.write_images(images, path).await?;
            }
            #[cfg(feature = "pdf")]
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

        let pages = episode.pages();
        let mut images = self
            .progress
            .build_with_message(pages.len(), "Downloading...")?
            .wrap_stream(stream::iter(pages))
            .enumerate()
            .map(|(i, page)| async move { Ok((i, self.fetch_image(&page).await?)) })
            .buffer_unordered(self.num_connections)
            .map_ok(|(i, image)| async move { Ok((i, self.solve_image(image, None).await?)) })
            .try_buffer_unordered(self.num_threads)
            .try_collect::<Vec<_>>()
            .await?;
        images.par_sort_by_key(|&(i, _)| i);
        let images = images
            .into_iter()
            .map(|(_, image)| image)
            .collect::<Vec<_>>();

        self.write_images(images, path).await?;
        Ok(())
    }

    async fn download_in<T: AsRef<Path>>(&self, url: &Url, dir: T) -> Result<()> {
        let episode_id = self.parse_episode_id(url)?;
        let episode = self.fetch_episode(&episode_id).await?;

        let mut path = dir
            .as_ref()
            .join(episode.title().context("Episode title not found")?);
        match self.writer_config.save_format() {
            SaveFormat::Raw => {} // Do nothing
            SaveFormat::Zip { .. } => {
                path.set_extension("zip");
            }
            #[cfg(feature = "pdf")]
            SaveFormat::Pdf => {
                path.set_extension("pdf");
            }
        }

        let pages = episode.pages();
        let mut images = self
            .progress
            .build_with_message(pages.len(), "Downloading...")?
            .wrap_stream(stream::iter(pages))
            .enumerate()
            .map(|(i, page)| async move { Ok((i, self.fetch_image(&page).await?)) })
            .buffer_unordered(self.num_connections)
            .map_ok(|(i, image)| async move { Ok((i, self.solve_image(image, None).await?)) })
            .try_buffer_unordered(self.num_threads)
            .try_collect::<Vec<_>>()
            .await?;
        images.par_sort_by_key(|&(i, _)| i);
        let images = images
            .into_iter()
            .map(|(_, image)| image)
            .collect::<Vec<_>>();

        self.write_images(images, path).await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use crate::viewer::ViewerWebsite;

    use super::*;

    #[tokio::test]
    async fn test_pipeline_download_raw() -> Result<()> {
        let url = Url::parse("https://shonenjumpplus.com/episode/16457717013869519536")?;
        let path = "playground/output/giga_pipe_raw";

        let pipe = Pipeline::default();

        pipe.download(&url, path).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_pipeline_download_zip() -> Result<()> {
        let url = Url::parse("https://shonenjumpplus.com/episode/16457717013869519536")?;
        let path = "playground/output/giga_pipe_zip.zip";

        let pipe = Pipeline::default().set_writer_config(WriterConifg::new(
            SaveFormat::Zip {
                compression_method: zip::CompressionMethod::Zstd,
                extension: None,
            },
            image::ImageFormat::WebP,
        ));

        pipe.download(&url, path).await?;
        Ok(())
    }

    #[cfg(feature = "pdf")]
    #[tokio::test]
    async fn test_pipeline_download_pdf() -> Result<()> {
        let url = Url::parse("https://shonenjumpplus.com/episode/16457717013869519536")?;
        let path = "playground/output/giga_pipe_pdf.pdf";

        let pipe = Pipeline::default()
            .set_writer_config(WriterConifg::new(SaveFormat::Pdf, image::ImageFormat::Jpeg));

        pipe.download(&url, path).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_pipeline_all_websites_zip() -> Result<()> {
        let dir = Path::new("output/giga_pipe_websites");

        tokio::fs::create_dir_all(dir).await?;

        let urls = vec![
            [
                "https://shonenjumpplus.com/episode/9324103625676410700",
                "shonenjumpplus",
            ],
            [
                "https://tonarinoyj.jp/episode/2550912964641693231",
                "tonarinoyj",
            ],
            [
                "https://pocket.shonenmagazine.com/episode/316112896949465972",
                "magapoke",
            ],
            [
                "https://comic-days.com/episode/2550912964485733650",
                "comic-days",
            ],
            [
                "https://kuragebunch.com/episode/2550912964645853115",
                "kuragebunch",
            ],
            [
                "https://viewer.heros-web.com/episode/3269632237305675090",
                "comic-heros",
            ],
            [
                "https://comicborder.com/episode/14079602755169791873",
                "comicborder",
            ],
            [
                "https://comic-gardo.com/episode/3269754496561199721",
                "comic-gardo",
            ],
            [
                "https://comic-zenon.com/episode/14079602755568010150",
                "comic-zenon",
            ],
            ["https://magcomi.com/episode/2550912964518979926", "magcomi"],
            [
                "https://comic-action.com/episode/13933686331665056851",
                "comic-action",
            ],
            [
                "https://comic-trail.com/episode/11341664176587944169",
                "comic-trail",
            ],
            [
                "https://comic-growl.com/episode/4856001361425577926",
                "comic-growl",
            ],
            ["https://feelweb.jp/episode/2550689798285500143", "feelweb"],
            [
                "https://www.sunday-webry.com/episode/3269754496548997914",
                "sunday-webry",
            ],
            [
                "https://comic-ogyaaa.com/episode/4856001361171643976",
                "comic-ogyaaa",
            ],
            [
                "https://comic-earthstar.com/episode/2550689798355026496",
                "comic-earthstar",
            ],
            ["https://ourfeel.jp/episode/2550689798871964571", "ourfeel"],
        ];

        let writer_config = Arc::new(WriterConifg::new(
            SaveFormat::Zip {
                compression_method: zip::CompressionMethod::Zstd,
                extension: Some("cbz".to_string()),
            },
            image::ImageFormat::WebP,
        ));

        stream::iter(urls)
            .map(|[url, name]| {
                let writer_config = writer_config.clone();

                tokio::spawn(async move {
                    let path = dir.join(name);
                    let url = Url::parse(url)?;
                    let host = url.host_str().context("Host not found")?;

                    let website =
                        Website::lookup(host).context(format!("Website not found: {}", url))?;

                    println!("Downloading from: {:?}", website);

                    let pipe = Pipeline::default()
                        .set_website(website)
                        .set_writer_config(writer_config.as_ref().clone());

                    pipe.download(&url, path).await?;

                    Ok(())
                })
            })
            .buffer_unordered(num_cpus::get())
            .try_collect::<Vec<_>>()
            .await?;
        Ok(())
    }
}
