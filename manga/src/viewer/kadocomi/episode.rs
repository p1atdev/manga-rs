use anyhow::{Context, Result};
use futures::{stream, StreamExt, TryStreamExt};
use image::DynamicImage;
use std::{path::Path, sync::Arc, usize};
use url::Url;

#[cfg(feature = "pdf")]
use crate::io::pdf::PdfWriter;
use crate::{
    data::MangaEpisode,
    error::ClientError,
    io::FileWriter,
    pipeline::{EpisodePipeline, EpisodePipelineBuilder, SaveFormat, WriterConifg},
    progress::ProgressConfig,
    solver::ImageSolver,
    utils::Bytes,
    viewer::{ViewerClient, ViewerConfigBuilder},
};

use super::{
    data::episode::{Episode, Page},
    pipeline::Pipeline,
    solver::Solver,
    viewer::{Client, ConfigBuilder, Website},
};

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
    async fn fetch_episode(&self, episode_id: &str) -> Result<Episode, ClientError> {
        self.client.get_api_viewer(episode_id).await
    }

    async fn fetch_image(&self, page: &Page) -> Result<Bytes, ClientError> {
        let client = self.client.clone();

        let url = page.url().map_err(|_| ClientError::InvalidPage)?;
        let res = client.get(url).await?;
        let bytes = res.bytes().await.map_err(|_| ClientError::DecodeError)?;

        Ok(bytes.into())
    }

    async fn solve_image_bytes(&self, image: Bytes, page: Option<Page>) -> Result<Bytes> {
        let page = page.context("Page is required to solve image")?;

        tokio::task::spawn_blocking(move || {
            let solver = Arc::new(Solver::from_hex(&page.encryption_key())?);
            let image = solver.solve(image)?;
            Ok(image)
        })
        .await?
    }

    async fn solve_image(&self, image: Bytes, page: Option<Page>) -> Result<DynamicImage> {
        let page = page.context("Page is required to solve image")?;

        tokio::task::spawn_blocking(move || {
            let solver = Arc::new(Solver::from_hex(&page.encryption_key())?);
            let image = solver.solve_from_bytes(image)?;
            Ok(image)
        })
        .await?
    }

    fn file_writer<P: AsRef<Path>>(&self, save_path: &P) -> Result<FileWriter> {
        FileWriter::new(&self.writer_config, save_path)
    }

    /// Download an episode to the specified path
    async fn download<P: AsRef<Path>>(&self, url: &Url, path: &P) -> Result<()> {
        let next_data = self.client.get_next_data(url.clone()).await?;
        let episode_id = next_data.episode_id()?;

        let episode = self.fetch_episode(&episode_id).await?;
        let writer = self.file_writer(&path)?;
        writer.prepare().await?;

        let pages = episode.pages();
        let progress = self.progress.build_with_message(
            pages.len(),
            format!(
                "Downloading {}...",
                episode.title().unwrap_or("Unknown episode".to_string())
            ),
        )?;

        stream::iter(pages)
            .enumerate()
            .map(|(i, page)| async move { Ok((i, self.fetch_image(&page).await?, page)) })
            .buffer_unordered(self.num_connections)
            .map_ok(|(i, image, page)| async move {
                Ok((i, self.solve_image(image, Some(page)).await?))
            })
            .try_buffer_unordered(self.num_threads)
            .map_ok(|(i, image)| {
                let writer = writer.clone();
                async move { Ok((i, self.write(&writer, i, image).await?)) }
            })
            .try_buffer_unordered(self.num_threads)
            .map_ok(|_| {
                progress.inc(1);
                async move { anyhow::Ok(()) }
            })
            .try_buffered(self.num_threads)
            .try_collect::<Vec<_>>()
            .await?;

        progress.finish();

        Ok(())
    }

    /// Download an episode into the specified directory
    async fn download_in<T: AsRef<Path>>(&self, url: &Url, dir: &T) -> Result<()> {
        let next_data = self.client.get_next_data(url.clone()).await?;
        let episode_id = next_data.episode_id()?;

        let episode = self.fetch_episode(&episode_id).await?;
        let title = next_data.episode_title()?;

        let mut path = dir.as_ref().join(title);
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

        let pages = episode.pages();
        let progress = self.progress.build_with_message(
            pages.len(),
            format!(
                "Downloading {}...",
                episode.title().unwrap_or("Unknown episode".to_string())
            ),
        )?;

        stream::iter(pages)
            .enumerate()
            .map(|(i, page)| async move { Ok((i, self.fetch_image(&page).await?, page)) })
            .buffer_unordered(self.num_connections)
            .map_ok(|(i, image, page)| async move {
                Ok((i, self.solve_image(image, Some(page)).await?))
            })
            .try_buffer_unordered(self.num_threads)
            .map_ok(|(i, image)| {
                let writer = writer.clone();
                async move { Ok((i, self.write(&writer, i, image).await?)) }
            })
            .try_buffer_unordered(self.num_threads)
            .map_ok(|_| {
                progress.inc(1);
                async move { anyhow::Ok(()) }
            })
            .try_buffered(self.num_threads)
            .try_collect::<Vec<_>>()
            .await?;

        progress.finish();

        Ok(())
    }
}

#[cfg(test)]
mod test {

    use anyhow::Ok;

    use super::*;

    #[tokio::test]
    async fn test_get_api_contents_viewer() -> Result<()> {
        let url = Url::parse("https://comic-walker.com/detail/KC_000735_S?episodeType=first")?;

        let pipe = Pipeline::default();

        let next_data = pipe.client.get_next_data(url.clone()).await?;
        let episode_id = next_data.episode_id()?;

        let _episode = pipe.fetch_episode(&episode_id).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_download_raw() -> Result<()> {
        let url = Url::parse("https://comic-walker.com/detail/KC_000735_S?episodeType=first")?;
        let path = "tests/output/kadocomi_pipe_raw";

        let pipe = Pipeline::default()
            .set_writer_config(WriterConifg::new(SaveFormat::Raw, image::ImageFormat::WebP));

        pipe.download(&url, &path).await?;

        Ok(())
    }
}
