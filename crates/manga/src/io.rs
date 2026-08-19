use std::{future::Future, path::Path};

use anyhow::Result;
use image::DynamicImage;

use crate::pipeline::{SaveFormat, WriterConfig};

pub mod raw;
pub mod zip;

/// A trait for saving manga to disk.
pub trait EpisodeWriter {
    fn save_path(&self) -> &Path;

    /// Prepare before writing
    fn prepare(&self) -> impl Future<Output = Result<()>> {
        async { Ok(()) }
    }

    /// Write a page to disk
    fn write_page(&self, page: usize, image: DynamicImage) -> impl Future<Output = Result<()>>;

    /// Finalize pending output.
    fn finish(&self) -> impl Future<Output = Result<()>> {
        async { Ok(()) }
    }
}

#[derive(Debug, Clone)]
pub enum FileWriter {
    Raw(raw::RawWriter),
    Zip(zip::ZipWriter),
}

impl FileWriter {
    pub fn new<P: AsRef<Path>>(writer_config: &WriterConfig, save_path: &P) -> Result<Self> {
        match writer_config.save_format() {
            SaveFormat::Raw => {
                let writer = raw::RawWriter::new(writer_config.image_format(), save_path);
                Ok(FileWriter::Raw(writer))
            }
            SaveFormat::Zip {
                compression_method,
                extension: _,
            } => {
                let writer = zip::ZipWriter::new(
                    *compression_method,
                    writer_config.image_format(),
                    save_path,
                )?;
                Ok(FileWriter::Zip(writer))
            }
        }
    }

    pub async fn prepare(&self) -> Result<()> {
        match self {
            FileWriter::Raw(writer) => writer.prepare().await?,
            FileWriter::Zip(writer) => writer.prepare().await?,
        }
        Ok(())
    }

    pub async fn write_page(&self, page: usize, image: DynamicImage) -> Result<()> {
        match self {
            FileWriter::Raw(writer) => writer.write_page(page, image).await,
            FileWriter::Zip(writer) => writer.write_page(page, image).await,
        }
    }

    pub async fn finish(&self) -> Result<()> {
        match self {
            FileWriter::Raw(writer) => writer.finish().await,
            FileWriter::Zip(writer) => writer.finish().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn test_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "manga-rs-writer-{}-{}",
            std::process::id(),
            TEST_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[tokio::test]
    async fn raw_writer_uses_lexically_sortable_page_names() -> Result<()> {
        let directory = test_directory();
        let config = WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png);
        let writer = FileWriter::new(&config, &directory)?;

        writer.prepare().await?;
        writer.write_page(7, DynamicImage::new_rgb8(2, 2)).await?;
        writer.finish().await?;

        assert!(directory.join("0007.png").is_file());
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[tokio::test]
    async fn zip_writer_finalizes_a_readable_archive() -> Result<()> {
        let directory = test_directory();
        let path = directory.join("episode.cbz");
        let config = WriterConfig::new(
            SaveFormat::Zip {
                compression_method: ::zip::CompressionMethod::Deflated,
                extension: Some("cbz".to_owned()),
            },
            image::ImageFormat::Png,
        );
        let writer = FileWriter::new(&config, &path)?;

        writer.prepare().await?;
        writer.write_page(12, DynamicImage::new_rgb8(2, 2)).await?;
        writer.finish().await?;

        let mut archive = ::zip::ZipArchive::new(File::open(&path)?)?;
        assert!(archive.by_name("0012.png")?.size() > 0);
        drop(archive);
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }
}
