use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use image::DynamicImage;
use tokio::sync::Mutex;
use zip::{
    CompressionMethod,
    write::{ExtendedFileOptions, FileOptions},
};

use crate::utils;

use super::EpisodeWriter;

/// Save as a zip file.
#[derive(Debug, Clone)]
pub struct ZipWriter {
    compression_method: CompressionMethod,
    image_format: image::ImageFormat,
    save_path: PathBuf,
    writer: Arc<Mutex<Option<zip::ZipWriter<std::fs::File>>>>,
}

impl ZipWriter {
    pub fn new<P: AsRef<Path>>(
        compression_method: CompressionMethod,
        image_format: image::ImageFormat,
        save_path: &P,
    ) -> Result<Self> {
        Ok(ZipWriter {
            compression_method,
            image_format,
            save_path: save_path.as_ref().to_path_buf(),
            writer: Arc::new(Mutex::new(None)),
        })
    }

    pub fn default<P: AsRef<Path>>(save_path: &P) -> Result<Self> {
        Ok(ZipWriter {
            compression_method: CompressionMethod::Zstd,
            image_format: image::ImageFormat::Png,
            save_path: save_path.as_ref().with_extension("zip"),
            writer: Arc::new(Mutex::new(None)),
        })
    }
}

impl EpisodeWriter for ZipWriter {
    fn save_path(&self) -> &Path {
        &self.save_path
    }

    async fn prepare(&self) -> Result<()> {
        // mkdir parent directory
        if let Some(parent) = self
            .save_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            tokio::fs::create_dir_all(parent).await?;
        }

        let save_path = self.save_path.clone();
        let zip = tokio::task::spawn_blocking(move || {
            let file = std::fs::File::create(save_path)?;
            Ok::<_, anyhow::Error>(zip::ZipWriter::new(file))
        })
        .await??;
        *self.writer.lock().await = Some(zip);

        Ok(())
    }

    async fn write_page(&self, page: usize, image: DynamicImage) -> Result<()> {
        let options = FileOptions::<ExtendedFileOptions>::default()
            .compression_method(self.compression_method);
        let image_format = self.image_format;
        let file_name = format!("{page:04}.{}", image_format.extensions_str()[0]);

        let byte = tokio::task::spawn_blocking(move || utils::encode_image(&image, image_format))
            .await??;
        let mut state = self.writer.clone().lock_owned().await;
        tokio::task::spawn_blocking(move || {
            let zip = state.as_mut().context("zip writer was not prepared")?;
            zip.start_file(file_name, options)?;
            zip.write_all(&byte)?;
            Ok(())
        })
        .await?
    }

    async fn finish(&self) -> Result<()> {
        let mut state = self.writer.clone().lock_owned().await;
        tokio::task::spawn_blocking(move || {
            let zip = state.take().context("zip writer was not prepared")?;
            zip.finish()?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
}
