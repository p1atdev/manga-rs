use std::{
    future::Future,
    path::{Path, PathBuf},
};

use anyhow::{ensure, Result};
use futures::{stream, StreamExt, TryStreamExt};
use image::DynamicImage;
use url::Url;

use crate::{
    data::{MangaEpisode, MangaPage},
    error::{ClientError, PipelineError},
    io::FileWriter,
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
}

/// Configuration for the writer
#[derive(Debug, Clone)]
pub struct WriterConfig {
    save_format: SaveFormat,
    image_format: image::ImageFormat,
}

impl WriterConfig {
    pub fn new(save_format: SaveFormat, image_format: image::ImageFormat) -> Self {
        WriterConfig {
            save_format,
            image_format,
        }
    }

    pub fn save_format(&self) -> SaveFormat {
        self.save_format.clone()
    }

    pub fn image_format(&self) -> image::ImageFormat {
        self.image_format
    }

    pub fn output_path<P: AsRef<Path>>(&self, directory: P, title: &str) -> Result<PathBuf> {
        let title = safe_file_name(title)?;
        let mut path = directory.as_ref().join(title);
        if let SaveFormat::Zip { extension, .. } = &self.save_format {
            path.set_extension(extension.as_deref().unwrap_or("zip"));
        }
        Ok(path)
    }
}

fn safe_file_name(title: &str) -> Result<String> {
    let sanitized = title
        .trim()
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();

    ensure!(
        !sanitized.is_empty() && sanitized != "." && sanitized != "..",
        "episode title cannot be used as a file name"
    );
    Ok(sanitized)
}

/// Pipeline configuration trait
pub trait EpisodePipelineBuilder<W, A: MangaPage, B: MangaEpisode<A>, P: EpisodePipeline<A, B>>:
    Default
{
    fn set_website(self, website: W) -> Self;
    fn set_progress(self, progress: ProgressConfig) -> Self;
    fn set_writer_config(self, writer_config: WriterConfig) -> Self;
    fn set_num_threads(self, num_threads: usize) -> Self;
    fn set_num_connections(self, num_connections: usize) -> Self;
}

/// Pipeline to download manga
#[allow(async_fn_in_trait)]
pub trait EpisodePipeline<P: MangaPage, E: MangaEpisode<P>> {
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

    fn progress(&self) -> &ProgressConfig;

    fn num_threads(&self) -> usize;

    fn num_connections(&self) -> usize;

    async fn download_pages(&self, pages: Vec<P>, writer: FileWriter, title: &str) -> Result<()>
    where
        Self: Sync,
        P: Send + 'static,
    {
        ensure!(
            self.num_connections() > 0,
            "num_connections must be greater than zero"
        );
        ensure!(
            self.num_threads() > 0,
            "num_threads must be greater than zero"
        );
        writer.prepare().await?;
        let progress = self
            .progress()
            .build_with_message(pages.len(), format!("Downloading {title}..."))?;
        let processing_result = stream::iter(pages)
            .enumerate()
            .map(|(index, page)| async move {
                let bytes = self.fetch_image(&page).await?;
                Ok::<_, anyhow::Error>((index, bytes, page))
            })
            .buffer_unordered(self.num_connections())
            .map_ok(|(index, bytes, page)| async move {
                let image = self.solve_image(bytes, Some(page)).await?;
                Ok::<_, anyhow::Error>((index, image))
            })
            .try_buffer_unordered(self.num_threads())
            .map_ok(|(index, image)| {
                let writer = writer.clone();
                async move {
                    writer.write_page(index, image).await?;
                    Ok::<_, anyhow::Error>(())
                }
            })
            .try_buffer_unordered(self.num_threads())
            .try_for_each(|()| {
                progress.inc(1);
                async { Ok(()) }
            })
            .await;

        let finish_result = writer.finish().await;
        progress.finish();

        processing_result?;
        finish_result
    }

    /// Just download in the specified path
    fn download<T: AsRef<Path>>(&self, url: &Url, path: &T) -> impl Future<Output = Result<()>>;

    /// Download with a new folder or file in the specified directory
    fn download_in<T: AsRef<Path>>(&self, url: &Url, dir: &T) -> impl Future<Output = Result<()>>;
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use anyhow::anyhow;

    use super::*;

    #[derive(Debug, Clone)]
    struct TestPage;

    impl MangaPage for TestPage {
        fn index(&self) -> Result<usize> {
            Ok(0)
        }

        fn is_image(&self) -> bool {
            true
        }
    }

    #[derive(Debug, Clone)]
    struct TestEpisode;

    impl MangaEpisode<TestPage> for TestEpisode {
        fn id(&self) -> String {
            "test".to_owned()
        }

        fn index(&self) -> crate::data::IndexNumber {
            crate::data::IndexNumber::Int(0)
        }

        fn title(&self) -> Option<String> {
            Some("test".to_owned())
        }

        fn pages(&self) -> Vec<TestPage> {
            vec![TestPage]
        }
    }

    struct FailingPipeline {
        progress: ProgressConfig,
        writer_config: WriterConfig,
        num_threads: usize,
        num_connections: usize,
    }

    impl EpisodePipeline<TestPage, TestEpisode> for FailingPipeline {
        async fn fetch_episode(&self, _episode_id: &str) -> Result<TestEpisode, ClientError> {
            Ok(TestEpisode)
        }

        async fn fetch_image(&self, _page: &TestPage) -> Result<Bytes, ClientError> {
            Err(ClientError::InvalidPage)
        }

        async fn solve_image_bytes(&self, _image: Bytes, _page: Option<TestPage>) -> Result<Bytes> {
            Err(anyhow!("not reached"))
        }

        async fn solve_image(
            &self,
            _image: Bytes,
            _page: Option<TestPage>,
        ) -> Result<DynamicImage> {
            Err(anyhow!("not reached"))
        }

        fn file_writer<T: AsRef<Path>>(&self, save_path: &T) -> Result<FileWriter> {
            FileWriter::new(&self.writer_config, save_path)
        }

        fn progress(&self) -> &ProgressConfig {
            &self.progress
        }

        fn num_threads(&self) -> usize {
            self.num_threads
        }

        fn num_connections(&self) -> usize {
            self.num_connections
        }

        async fn download<T: AsRef<Path>>(&self, _url: &Url, _path: &T) -> Result<()> {
            Err(anyhow!("not used"))
        }

        async fn download_in<T: AsRef<Path>>(&self, _url: &Url, _dir: &T) -> Result<()> {
            Err(anyhow!("not used"))
        }
    }

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn test_path(extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "manga-rs-pipeline-{}-{}.{}",
            std::process::id(),
            TEST_ID.fetch_add(1, Ordering::Relaxed),
            extension
        ))
    }

    #[test]
    fn writer_config_builds_raw_directory_path() {
        let config = WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png);
        assert_eq!(
            config.output_path("out", "Episode 1").unwrap(),
            PathBuf::from("out/Episode 1")
        );
    }

    #[test]
    fn writer_config_uses_configured_archive_extension() {
        let config = WriterConfig::new(
            SaveFormat::Zip {
                compression_method: zip::CompressionMethod::Deflated,
                extension: Some("cbz".to_owned()),
            },
            image::ImageFormat::WebP,
        );
        assert_eq!(
            config.output_path("out", "Episode 1").unwrap(),
            PathBuf::from("out/Episode 1.cbz")
        );
    }

    #[test]
    fn writer_config_neutralizes_path_separators() {
        let config = WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png);
        assert_eq!(
            config.output_path("out", "../../escape\\name").unwrap(),
            PathBuf::from("out/.._.._escape_name")
        );
        assert!(config.output_path("out", "..").is_err());
    }

    #[tokio::test]
    async fn rejects_zero_concurrency_before_creating_output() {
        let path = test_path("raw");
        let pipeline = FailingPipeline {
            progress: ProgressConfig::disabled(),
            writer_config: WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png),
            num_threads: 1,
            num_connections: 0,
        };
        let writer = pipeline.file_writer(&path).unwrap();

        let error = pipeline
            .download_pages(Vec::new(), writer, "test")
            .await
            .unwrap_err();

        assert!(error.to_string().contains("num_connections"));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn finalizes_zip_after_processing_error() {
        let path = test_path("zip");
        let pipeline = FailingPipeline {
            progress: ProgressConfig::disabled(),
            writer_config: WriterConfig::new(
                SaveFormat::Zip {
                    compression_method: ::zip::CompressionMethod::Deflated,
                    extension: None,
                },
                image::ImageFormat::Png,
            ),
            num_threads: 1,
            num_connections: 1,
        };
        let writer = pipeline.file_writer(&path).unwrap();

        assert!(pipeline
            .download_pages(vec![TestPage], writer, "test")
            .await
            .is_err());

        let archive = ::zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert!(archive.is_empty());
        std::fs::remove_file(path).unwrap();
    }
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
    fn get_episode_queue(&self, url: Url) -> impl Future<Output = Result<Vec<EpisodeQueueItem>>>;

    /// Download with a new folder or file in the specified directory
    fn download_episode<T: AsRef<Path>>(
        &self,
        url: &Url,
        path: &T,
    ) -> impl Future<Output = Result<(), PipelineError>>;

    /// Download multiple episodes specified by the urls
    fn download_series<T: AsRef<Path>>(
        &self,
        url: &Url,
        dir: &T,
    ) -> impl Future<Output = Result<()>>;
}
