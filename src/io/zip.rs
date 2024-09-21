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

    /// Save images as a zip file.
    pub async fn write<P: AsRef<Path>>(&self, images: Vec<DynamicImage>, path: P) -> Result<()> {
        let file = std::fs::File::create(path.as_ref())?;

        let zip = Arc::new(Mutex::new(zip::ZipWriter::new(file)));
        let image_format = self.image_format;
        let compression_method = self.compression_method;

        let images_len = images.len();
        let mut tasks = vec![];
        for (i, image) in images
            .into_iter()
            .enumerate()
            .progress_with(self.progress.build(images_len)?)
        {
            let zip = zip.clone();
            let task = tokio::spawn(async move {
                let options = FileOptions::<ExtendedFileOptions>::default()
                    .compression_method(compression_method);
                let mut bytes: Vec<u8> = Vec::new();
                image.write_to(&mut Cursor::new(&mut bytes), image_format)?;

                let mut zip = zip.lock().await;
                zip.start_file(
                    format!("{}.{}", i, image_format.extensions_str()[0]),
                    options,
                )?;
                zip.write_all(&bytes)?;

                Result::<()>::Ok(())
            });
            tasks.push(task);
        }

        self.progress
            .build(tasks.len())?
            .wrap_stream(stream::iter(tasks))
            .buffer_unordered(self.num_threads)
            .collect::<Vec<_>>()
            .await;

        Ok(())
    }
}
