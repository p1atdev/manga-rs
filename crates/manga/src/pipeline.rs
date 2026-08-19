use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, ensure};
use futures::{StreamExt, stream};
use image::DynamicImage;
use tokio::sync::Semaphore;
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

    pub fn save_format(&self) -> &SaveFormat {
        &self.save_format
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

/// Shared limits for all downloads started by a pipeline.
#[derive(Debug, Clone)]
pub struct DownloadLimits {
    num_threads: usize,
    num_connections: usize,
    in_flight: Arc<OnceLock<Semaphore>>,
    processing: Arc<OnceLock<Semaphore>>,
    connections: Arc<OnceLock<Semaphore>>,
}

impl DownloadLimits {
    pub fn new(num_threads: usize, num_connections: usize) -> Self {
        Self {
            num_threads,
            num_connections,
            in_flight: Arc::new(OnceLock::new()),
            processing: Arc::new(OnceLock::new()),
            connections: Arc::new(OnceLock::new()),
        }
    }

    pub fn num_threads(&self) -> usize {
        self.num_threads
    }

    pub fn num_connections(&self) -> usize {
        self.num_connections
    }

    pub fn set_num_threads(&mut self, num_threads: usize) {
        self.num_threads = num_threads;
        self.in_flight = Arc::new(OnceLock::new());
        self.processing = Arc::new(OnceLock::new());
    }

    pub fn set_num_connections(&mut self, num_connections: usize) {
        self.num_connections = num_connections;
        self.in_flight = Arc::new(OnceLock::new());
        self.connections = Arc::new(OnceLock::new());
    }

    pub(crate) fn validate(&self) -> Result<usize> {
        ensure!(
            self.num_connections > 0,
            "num_connections must be greater than zero"
        );
        ensure!(
            self.num_threads > 0,
            "num_threads must be greater than zero"
        );
        let max_in_flight = self
            .num_connections
            .checked_add(self.num_threads)
            .context("combined download concurrency is too large")?;
        ensure!(
            max_in_flight <= Semaphore::MAX_PERMITS,
            "combined download concurrency exceeds the supported limit"
        );
        Ok(max_in_flight)
    }

    fn in_flight(&self, max_in_flight: usize) -> &Semaphore {
        self.in_flight.get_or_init(|| Semaphore::new(max_in_flight))
    }

    fn processing(&self) -> &Semaphore {
        self.processing
            .get_or_init(|| Semaphore::new(self.num_threads))
    }

    fn connections(&self) -> &Semaphore {
        self.connections
            .get_or_init(|| Semaphore::new(self.num_connections))
    }

    pub(crate) async fn acquire_connection(&self) -> Result<tokio::sync::SemaphorePermit<'_>> {
        self.connections()
            .acquire()
            .await
            .context("download connection limiter was closed")
    }
}

enum PageDownloadResult {
    Downloaded,
    Cancelled,
    Failed(anyhow::Error),
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

/// Pipeline configuration trait.
pub trait EpisodePipelineBuilder: Sized {
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

    fn download_limits(&self) -> &DownloadLimits;

    async fn download_pages(&self, pages: Vec<P>, writer: FileWriter, title: &str) -> Result<()>
    where
        Self: Sync,
        P: Send + 'static,
    {
        let limits = self.download_limits();
        let max_in_flight = limits.validate()?;
        let progress = self
            .progress()
            .build_with_message(pages.len(), format!("Downloading {title}..."))?;
        writer.prepare().await?;
        let cancelled = AtomicBool::new(false);
        let processing_error = stream::iter(pages)
            .enumerate()
            .map(|(index, page)| {
                let cancelled = &cancelled;
                let writer = &writer;
                async move {
                    if cancelled.load(Ordering::Acquire) {
                        return PageDownloadResult::Cancelled;
                    }

                    let result = async {
                        let in_flight_permit = limits
                            .in_flight(max_in_flight)
                            .acquire()
                            .await
                            .context("in-flight page limiter was closed")?;
                        if cancelled.load(Ordering::Acquire) {
                            return Ok(None);
                        }
                        let bytes = {
                            let connection_permit = limits.acquire_connection().await?;
                            if cancelled.load(Ordering::Acquire) {
                                return Ok(None);
                            }
                            let bytes = self.fetch_image(&page).await?;
                            drop(connection_permit);
                            bytes
                        };

                        let processing_permit = limits
                            .processing()
                            .acquire()
                            .await
                            .context("image processing limiter was closed")?;
                        if cancelled.load(Ordering::Acquire) {
                            return Ok(None);
                        }
                        let image = self.solve_image(bytes, Some(page)).await?;
                        if cancelled.load(Ordering::Acquire) {
                            return Ok(None);
                        }
                        writer.write_page(index, image).await?;
                        drop(processing_permit);
                        drop(in_flight_permit);

                        Ok::<_, anyhow::Error>(Some(()))
                    }
                    .await;

                    match result {
                        Ok(Some(())) => PageDownloadResult::Downloaded,
                        Ok(None) => PageDownloadResult::Cancelled,
                        Err(error) => {
                            cancelled.store(true, Ordering::Release);
                            PageDownloadResult::Failed(error)
                        }
                    }
                }
            })
            .buffer_unordered(max_in_flight)
            .fold(None, |first_error, result| {
                let next_error = match result {
                    PageDownloadResult::Downloaded => {
                        progress.inc(1);
                        first_error
                    }
                    PageDownloadResult::Cancelled => first_error,
                    PageDownloadResult::Failed(error) => first_error.or(Some(error)),
                };
                async move { next_error }
            })
            .await;

        let finish_result = writer.finish().await;
        progress.finish();

        if let Some(error) = processing_error {
            return Err(error);
        }
        finish_result
    }

    /// Just download in the specified path
    fn download<T: AsRef<Path>>(&self, url: &Url, path: &T) -> impl Future<Output = Result<()>>;

    /// Download with a new folder or file in the specified directory
    fn download_in<T: AsRef<Path>>(&self, url: &Url, dir: &T) -> impl Future<Output = Result<()>>;
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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

        fn pages(&self) -> &[TestPage] {
            std::slice::from_ref(&TestPage)
        }

        fn into_pages(self) -> Vec<TestPage> {
            vec![TestPage]
        }
    }

    struct FailingPipeline {
        progress: ProgressConfig,
        writer_config: WriterConfig,
        limits: DownloadLimits,
        fetch_count: AtomicUsize,
    }

    impl EpisodePipeline<TestPage, TestEpisode> for FailingPipeline {
        async fn fetch_episode(&self, _episode_id: &str) -> Result<TestEpisode, ClientError> {
            Ok(TestEpisode)
        }

        async fn fetch_image(&self, _page: &TestPage) -> Result<Bytes, ClientError> {
            self.fetch_count.fetch_add(1, Ordering::Relaxed);
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

        fn download_limits(&self) -> &DownloadLimits {
            &self.limits
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
            limits: DownloadLimits::new(1, 0),
            fetch_count: AtomicUsize::new(0),
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
            limits: DownloadLimits::new(1, 1),
            fetch_count: AtomicUsize::new(0),
        };
        let writer = pipeline.file_writer(&path).unwrap();

        assert!(
            pipeline
                .download_pages(vec![TestPage], writer, "test")
                .await
                .is_err()
        );

        let archive = ::zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert!(archive.is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn stops_starting_pages_after_first_error() {
        let path = test_path("raw");
        let pipeline = FailingPipeline {
            progress: ProgressConfig::disabled(),
            writer_config: WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png),
            limits: DownloadLimits::new(2, 2),
            fetch_count: AtomicUsize::new(0),
        };
        let writer = pipeline.file_writer(&path).unwrap();

        assert!(
            pipeline
                .download_pages(vec![TestPage; 100], writer, "test")
                .await
                .is_err()
        );
        assert!(pipeline.fetch_count.load(Ordering::Relaxed) <= 4);

        std::fs::remove_dir_all(path).unwrap();
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
