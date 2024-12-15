use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::Result;
use image::DynamicImage;
use zip::{
    write::{ExtendedFileOptions, FileOptions},
    CompressionMethod,
};

use crate::utils;

use super::EpisodeWriter;

/// Save as a zip file.
#[derive(Debug, Clone)]
pub struct ZipWriter {
    compression_method: CompressionMethod,
    image_format: image::ImageFormat,
    save_path: PathBuf,
    writer: Arc<RwLock<zip::ZipWriter<std::fs::File>>>,
}

impl ZipWriter {
    pub fn new<P: AsRef<Path>>(
        compression_method: CompressionMethod,
        image_format: image::ImageFormat,
        extension: Option<String>,
        save_path: &P,
    ) -> Result<Self> {
        let file = std::fs::File::create(
            save_path
                .as_ref()
                .with_extension(extension.clone().unwrap_or("zip".to_string())),
        )?;
        let writer = Arc::new(RwLock::new(zip::ZipWriter::new(file)));

        Ok(ZipWriter {
            compression_method,
            image_format,
            save_path: save_path.as_ref().to_path_buf(),
            writer,
        })
    }

    pub fn default<P: AsRef<Path>>(save_path: &P) -> Result<Self> {
        let file = std::fs::File::create(save_path.as_ref().with_extension("zip".to_string()))?;
        let writer = Arc::new(RwLock::new(zip::ZipWriter::new(file)));

        Ok(ZipWriter {
            compression_method: CompressionMethod::Zstd,
            image_format: image::ImageFormat::Png,
            save_path: save_path.as_ref().to_path_buf(),
            writer,
        })
    }
}

impl EpisodeWriter for ZipWriter {
    fn save_path(&self) -> PathBuf {
        self.save_path.clone()
    }

    async fn prepare(&self) -> Result<()> {
        // mkdir parent directory
        let parent = self.save_path.parent().unwrap();
        tokio::fs::create_dir_all(parent).await?;

        Ok(())
    }

    async fn write_page(&self, page: usize, image: DynamicImage) -> Result<()> {
        let options = FileOptions::<ExtendedFileOptions>::default()
            .compression_method(self.compression_method);
        let image_format = self.image_format.clone();
        let writer = self.writer.clone();
        let file_name = format!("{}.{}", page, image_format.extensions_str()[0]);

        tokio::task::spawn_blocking(move || {
            let byte = utils::encode_image(&image, image_format)?;
            let mut zip = writer.write().unwrap();
            zip.start_file(file_name, options)?;
            zip.write_all(&byte)?;
            Ok(())
        })
        .await?
    }
}
