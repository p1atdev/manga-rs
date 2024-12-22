use anyhow::{bail, Context, Result};
use futures::{
    stream::{self, Iter},
    StreamExt, TryStreamExt,
};
use indicatif::{MultiProgress, ProgressBar};

use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
    sync::Arc,
};
use url::Url;

use crate::{
    data::MangaEpisode,
    error::{ClientError, PipelineError},
    pipeline::{EpisodePipeline, EpisodeQueueItem, SaveFormat, SeriesPipeline},
};

use super::{
    data::{Episode, Page},
    pipeline::Pipeline,
};

impl Pipeline {
    fn get_save_path(&self, directory: PathBuf, filename: &str) -> PathBuf {
        let mut path = directory.join(filename);
        match self.writer_config.save_format() {
            SaveFormat::Raw => {} // Do nothing
            SaveFormat::Zip { extension, .. } => {
                path.set_extension(extension.unwrap_or("zip".to_string()));
            }
            #[cfg(feature = "pdf")]
            SaveFormat::Pdf => {
                path.set_extension("pdf");
            }
        }
        path
    }
}

impl SeriesPipeline<Page, Episode> for Pipeline {
    async fn get_episode_queue(&self, url: Url) -> Result<Vec<EpisodeQueueItem>> {
        let series_id = self.client.get_series_id(url).await?;
        let feed = self.client.get_series_feed(&series_id).await?;

        let mut entries = feed.entries();
        entries
            // earlier is former
            .sort_by(|a, b| {
                if let Some(a) = a.updated() {
                    if let Some(b) = b.updated() {
                        return a.cmp(&b);
                    }
                }
                return Ordering::Equal;
            });

        entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let title = entry.title().unwrap_or(format!("Unknown Episode ({})", i));
                let links = entry.links();
                let links = links
                    .iter()
                    .filter(|link| link.rel() != Some("enclosure".to_string()))
                    .collect::<Vec<_>>();
                let link = links.first().context("No links found")?;
                let url = Url::parse(&link.url()).context("Failed to parse URL")?;
                Ok(EpisodeQueueItem::new(&title, url))
            })
            .collect()
    }

    async fn download_episode<T: AsRef<Path>>(
        &self,
        url: &Url,
        path: &T,
        progress: Arc<ProgressBar>,
    ) -> Result<(), PipelineError> {
        let episode_id = self
            .parse_episode_id(url)
            .map_err(|_| PipelineError::Unknown)?;
        let episode = self.fetch_episode(&episode_id).await?;

        let writer = Arc::new(
            self.file_writer(&path)
                .map_err(|_| PipelineError::IoError)?,
        );
        writer.prepare().await.map_err(|_| PipelineError::IoError)?;

        let pages = episode.pages();
        progress.set_length(pages.len() as u64);

        stream::iter(pages)
            .enumerate()
            .map(|(i, page)| async move { Ok((i, self.fetch_image(&page).await?)) })
            .buffer_unordered(self.num_connections)
            .map_ok(|(i, image)| async move {
                Ok((
                    i,
                    self.solve_image(image, None)
                        .await
                        .map_err(|_| PipelineError::SolveError)?,
                ))
            })
            .try_buffer_unordered(self.num_threads)
            .map_ok(|(i, image)| {
                let writer = writer.clone();
                async move {
                    Ok((
                        i,
                        self.write(&writer, i, image)
                            .await
                            .map_err(|_| PipelineError::IoError)?,
                    ))
                }
            })
            .try_buffer_unordered(self.num_threads)
            .map_ok(|_| {
                progress.inc(1);
                async move { Ok::<(), PipelineError>(()) }
            })
            .try_buffered(self.num_threads)
            .try_collect::<Vec<_>>()
            .await?;
        Ok(())
    }

    async fn download_series<T: AsRef<Path>>(&self, url: &Url, dir: &T) -> Result<()> {
        let queue = self.get_episode_queue(url.clone()).await?;
        let multibar = MultiProgress::new();
        let multibar = Arc::new(multibar);

        stream::iter(queue)
            .map(|item| {
                let url = item.url().clone();
                let title = item.title();

                let save_path = self.get_save_path(dir.as_ref().to_path_buf(), &title);
                let multibar = multibar.clone();
                let progress = self
                    .progress
                    .build(0)
                    .map_err(|_| PipelineError::ProgressError)?;
                progress.set_message(format!("Downloading {}...", title));
                let progress = multibar.add(progress);
                let progress = Arc::new(progress);

                Ok((url, save_path, progress))
            })
            .map_ok(|(url, path, progress)| async move {
                let res = self.download_episode(&url, &path, progress).await;
                if let Err(e) = res {
                    match &e {
                        PipelineError::ClientError(e) => match e {
                            ClientError::ParseError(msg) => {
                                eprintln!("Failed to download episode: Parse error {}", msg);
                                return Ok(());
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    return Err(e);
                }

                Ok(())
            })
            .try_buffer_unordered(self.num_connections)
            .try_collect::<Vec<_>>()
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_episode_urls() -> Result<()> {
        let pipeline = Pipeline::default();
        let url = Url::parse("https://shonenjumpplus.com/episode/3270375685341574016")?;
        let episodes = pipeline.get_episode_queue(url).await?;
        assert!(episodes.len() > 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_download_series() -> Result<()> {
        let pipeline = Pipeline::default();
        let url = Url::parse("https://shonenjumpplus.com/episode/17106371875032400936")?;
        let path = Path::new("tests/output/giga_pipe_series");
        pipeline.download_series(&url, &path).await?;

        Ok(())
    }
}
