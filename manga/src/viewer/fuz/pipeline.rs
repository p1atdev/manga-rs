use std::path::Path;

use anyhow::{bail, Context, Ok, Result};
use futures::{stream, StreamExt, TryStreamExt};
use image::DynamicImage;
use url::Url;

#[cfg(feature = "pdf")]
use crate::io::pdf::PdfWriter;
use crate::{
    data::{MangaEpisode, MangaPage},
    io::FileWriter,
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
    // file_writer: Arc<FileWriter>,
}

impl Default for Pipeline {
    fn default() -> Self {
        let writer_config = WriterConifg::new(SaveFormat::Raw, image::ImageFormat::Png);
        Self {
            client: Client::new(ConfigBuilder::new(Website::ComicFuz).build()),
            progress: ProgressConfig::default(),
            num_threads: num_cpus::get(),
            num_connections: 8,
            // file_writer: Arc::new(FileWriter::new(&writer_config, &"./").unwrap()),
            writer_config, // must be after file_writer
        }
    }
}

impl Pipeline {
    pub fn new<P: AsRef<Path>>(
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
            num_threads,
            num_connections,
            // file_writer: Arc::new(FileWriter::new(&writer_config, save_path).unwrap()), // TODO: remove unwrap
            writer_config,
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
        let url = self.client.image_url(page.image_path()?)?;
        let res = self.client.get(url).await?;
        let bytes = res.bytes().await?;

        Ok(bytes.into())
    }

    async fn solve_image_bytes(&self, bytes: Bytes, page: Option<Page>) -> Result<Bytes> {
        let page = page.context("Page is required to solve image")?;

        if let Page::Image(image_page) = page {
            let solver = Solver::new(image_page.encryption_key(), image_page.encryption_iv());
            let image = solver.solve(bytes)?;
            Ok(image)
        } else {
            bail!("Page is not an image")
        }
    }

    async fn solve_image(&self, bytes: Bytes, page: Option<Page>) -> Result<DynamicImage> {
        let page = page.context("Page is required to solve image")?;

        if let Page::Image(image_page) = page {
            tokio::task::spawn_blocking(move || {
                let solver = Solver::new(image_page.encryption_key(), image_page.encryption_iv());
                let image = solver.solve_from_bytes(bytes)?;
                Ok(image)
            })
            .await?
        } else {
            bail!("Page is not an image")
        }
    }

    fn file_writer<P: AsRef<Path>>(&self, path: &P) -> Result<FileWriter> {
        FileWriter::new(&self.writer_config, path)
    }

    async fn download<T: AsRef<Path>>(&self, url: &Url, path: &T) -> Result<()> {
        let episode_id = self.parse_episode_id(url)?;
        let episode = self.fetch_episode(&episode_id).await?;
        let pages = episode
            .pages()
            .into_iter()
            .filter(|page| page.is_image())
            .collect::<Vec<_>>();
        let writer = self.file_writer(path)?;
        writer.prepare().await?;

        self.progress
            .build_with_message(
                pages.len(),
                format!(
                    "Downloading {}...",
                    episode.title().unwrap_or("Unknown episode".to_string())
                ),
            )?
            .wrap_stream(stream::iter(pages))
            .enumerate()
            .map(|(i, page)| async move { Ok((i, page.clone(), self.fetch_image(&page).await?)) })
            .buffer_unordered(self.num_connections)
            .map_ok(|(i, page, image)| async move {
                Ok((i, self.solve_image(image, Some(page)).await?))
            })
            .try_buffer_unordered(self.num_threads)
            .map_ok(|(i, image)| {
                let writer = writer.clone();
                async move { Ok((i, self.write(&writer, i, image).await?)) }
            })
            .try_buffer_unordered(self.num_threads)
            .try_collect::<Vec<_>>()
            .await?;

        Ok(())
    }

    async fn download_in<T: AsRef<Path>>(&self, url: &Url, dir: &T) -> Result<()> {
        let episode_id = self.parse_episode_id(url)?;
        let episode = self.fetch_episode(&episode_id).await?;

        let mut path = dir.as_ref().join(
            episode
                .title()
                .context("Episode title not found")?
                .replace(".", "_"),
        );
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
        let writer = self.file_writer(&path)?;
        writer.prepare().await?;

        let pages = episode
            .pages()
            .into_iter()
            .filter(|page| page.is_image())
            .collect::<Vec<_>>();

        self.progress
            .build_with_message(
                pages.len(),
                format!(
                    "Downloading {}...",
                    episode.title().unwrap_or("Unknown episode".to_string())
                ),
            )?
            .wrap_stream(stream::iter(pages))
            .enumerate()
            .map(|(i, page)| async move { Ok((i, page.clone(), self.fetch_image(&page).await?)) })
            .buffer_unordered(self.num_connections)
            .map_ok(|(i, page, image)| async move {
                Ok((i, self.solve_image(image, Some(page)).await?))
            })
            .try_buffer_unordered(self.num_threads)
            .map_ok(|(i, image)| {
                let writer = writer.clone();
                async move { Ok((i, self.write(&writer, i, image).await?)) }
            })
            .try_buffer_unordered(self.num_threads)
            .try_collect::<Vec<_>>()
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_download_raw() -> Result<()> {
        let url = Url::parse("https://comic-fuz.com/manga/viewer/44994")?;
        let path = "tests/output/fuz_pipe_raw";

        let pipe = Pipeline::default().set_num_threads(16);

        pipe.download(&url, &path).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_pipeline_download_zip() -> Result<()> {
        let url = Url::parse("https://comic-fuz.com/manga/viewer/44994")?;
        let path = "tests/output/fuz_pipe_zip.zip";

        let pipe = Pipeline::default()
            .set_num_threads(16)
            .set_writer_config(WriterConifg::new(
                SaveFormat::Zip {
                    compression_method: zip::CompressionMethod::Deflated,
                    extension: None,
                },
                image::ImageFormat::WebP,
            ));

        pipe.download(&url, &path).await?;
        Ok(())
    }

    #[cfg(feature = "pdf")]
    #[tokio::test]
    async fn test_pipeline_download_pdf() -> Result<()> {
        let url = Url::parse("https://comic-fuz.com/manga/viewer/44994")?;
        let path = "tests/output/fuz_pipe_pdf.pdf";

        let pipe = Pipeline::default()
            .set_writer_config(WriterConifg::new(SaveFormat::Pdf, image::ImageFormat::Jpeg));

        pipe.download(&url, path).await?;
        Ok(())
    }
}
