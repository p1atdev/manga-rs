use std::{
    io::{Cursor, Write},
    path::Path,
    sync::Arc,
};

use anyhow::Result;
use futures::{stream, StreamExt};
use image::DynamicImage;
use indicatif::ProgressIterator;
use tokio::sync::Mutex;
use zip::{
    write::{ExtendedFileOptions, FileOptions},
    CompressionMethod,
};

use crate::progress::ProgressConfig;

use super::EpisodeWriter;

/// Save as a zip file.
#[derive(Debug, Clone)]
pub struct ZipWriter {
    compression_method: CompressionMethod,
    image_format: image::ImageFormat,
    num_threads: usize,
    progress: ProgressConfig,
}

impl ZipWriter {
    pub fn default() -> Self {
        ZipWriter {
            compression_method: CompressionMethod::Zstd,
            image_format: image::ImageFormat::Png,
            num_threads: num_cpus::get(),
            progress: ProgressConfig::default(),
        }
    }

    pub fn new(
        compression_method: CompressionMethod,
        image_format: image::ImageFormat,
        num_threads: usize,
        progress: ProgressConfig,
    ) -> Self {
        ZipWriter {
            compression_method,
            image_format,
            num_threads,
            progress,
        }
    }
}

impl EpisodeWriter for ZipWriter {
    /// Save images as a zip file.
    async fn write<P: AsRef<Path>>(&self, images: Vec<DynamicImage>, path: P) -> Result<()> {
        let file = std::fs::File::create(path.as_ref())?;

        let zip = Arc::new(Mutex::new(zip::ZipWriter::new(file)));
        let image_format = self.image_format;
        let compression_method = self.compression_method;

        self.progress
            .build(images.len())?
            .wrap_stream(futures::stream::iter(images))
            .enumerate()
            .map(|(i, image)| {
                tokio::task::spawn_blocking(move || {
                    let mut bytes: Vec<u8> = Vec::new();
                    image.write_to(&mut Cursor::new(&mut bytes), image_format)?;
                    Result::<_>::Ok((i, bytes))
                })
            })
            .buffer_unordered(self.num_threads)
            .map(|pair| pair?)
            .map(|pair| {
                let zip = zip.clone();
                let options = FileOptions::<ExtendedFileOptions>::default()
                    .compression_method(compression_method);
                tokio::spawn(async move {
                    let (i, bytes) = pair?;
                    let mut zip = zip.lock().await;
                    zip.start_file(
                        format!("{}.{}", i, image_format.extensions_str()[0]),
                        options,
                    )?;
                    zip.write_all(&bytes)?;
                    Result::<_>::Ok(())
                })
            })
            .buffer_unordered(self.num_threads)
            .collect::<Vec<_>>()
            .await;

        Ok(())
    }
}
