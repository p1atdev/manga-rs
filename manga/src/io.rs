use std::{
    future::Future,
    path::{Path, PathBuf},
};

use anyhow::Result;
use image::DynamicImage;

use crate::pipeline::{SaveFormat, WriterConifg};

#[cfg(feature = "pdf")]
pub mod pdf;
pub mod raw;
pub mod zip;

/// A trait for saving manga to disk.
pub trait EpisodeWriter {
    fn save_path(&self) -> PathBuf;

    /// Prepare before writing
    fn prepare(&self) -> impl Future<Output = Result<()>> {
        async { Ok(()) }
    }

    /// Write a page to disk
    fn write_page(&self, page: usize, image: DynamicImage) -> impl Future<Output = Result<()>>;
}

#[derive(Debug, Clone)]
pub enum FileWriter {
    Raw(raw::RawWriter),
    Zip(zip::ZipWriter),
    #[cfg(feature = "pdf")]
    Pdf(pdf::PdfWriter),
}

impl FileWriter {
    pub fn new<P: AsRef<Path>>(writer_config: &WriterConifg, save_path: &P) -> Result<Self> {
        match writer_config.save_format() {
            SaveFormat::Raw => {
                let writer = raw::RawWriter::new(writer_config.image_format(), save_path);
                return Ok(FileWriter::Raw(writer));
            }
            SaveFormat::Zip {
                compression_method,
                extension,
            } => {
                let writer = zip::ZipWriter::new(
                    compression_method,
                    writer_config.image_format(),
                    extension,
                    save_path,
                )?;
                return Ok(FileWriter::Zip(writer));
            }
            #[cfg(feature = "pdf")]
            SaveFormat::Pdf => {
                let writer = pdf::PdfWriter::new(writer_config.image_format());
                return Ok(FileWriter::Pdf(writer));
            }
        }
    }

    pub async fn prepare(&self) -> Result<()> {
        Ok(match self {
            FileWriter::Raw(writer) => writer.prepare().await?,
            FileWriter::Zip(writer) => writer.prepare().await?,
            #[cfg(feature = "pdf")]
            FileWriter::Pdf(writer) => writer.prepare(),
        })
    }
}
