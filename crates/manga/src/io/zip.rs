use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use image::DynamicImage;
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
    fn save_path(&self) -> PathBuf {
        self.save_path.clone()
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
        let writer = self.writer.clone();
        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::create(save_path)?;
            let mut state = writer
                .lock()
                .map_err(|_| anyhow::anyhow!("zip writer lock poisoned"))?;
            *state = Some(zip::ZipWriter::new(file));
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    async fn write_page(&self, page: usize, image: DynamicImage) -> Result<()> {
        let options = FileOptions::<ExtendedFileOptions>::default()
            .compression_method(self.compression_method);
        let image_format = self.image_format;
        let writer = self.writer.clone();
        let file_name = format!("{page:04}.{}", image_format.extensions_str()[0]);

        tokio::task::spawn_blocking(move || {
            let byte = utils::encode_image(&image, image_format)?;
            let mut state = writer
                .lock()
                .map_err(|_| anyhow::anyhow!("zip writer lock poisoned"))?;
            let zip = state.as_mut().context("zip writer was not prepared")?;
            zip.start_file(file_name, options)?;
            zip.write_all(&byte)?;
            Ok(())
        })
        .await?
    }

    async fn finish(&self) -> Result<()> {
        let writer = self.writer.clone();
        tokio::task::spawn_blocking(move || {
            let mut state = writer
                .lock()
                .map_err(|_| anyhow::anyhow!("zip writer lock poisoned"))?;
            let zip = state.take().context("zip writer was not prepared")?;
            zip.finish()?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
}
