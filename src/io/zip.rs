use std::{io::Write, path::Path, sync::Arc};

use anyhow::Result;
use futures::StreamExt;
use image::DynamicImage;
use tokio::sync::Mutex;
use zip::{
    write::{ExtendedFileOptions, FileOptions},
    CompressionMethod,
};

use crate::{progress::ProgressConfig, utils};

use super::EpisodeWriter;

/// Save as a zip file.
#[derive(Debug, Clone)]
pub struct ZipWriter {
    compression_method: CompressionMethod,
    image_format: image::ImageFormat,
    num_threads: usize,
    progress: ProgressConfig,
    // writer: Arc<Mutex<zip::ZipWriter<std::fs::File>>>,
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
    async fn write<P: AsRef<Path>, B: AsRef<[u8]>>(&self, images: Vec<B>, path: P) -> Result<()> {
        let file = std::fs::File::create(path.as_ref())?;
        let zip = Arc::new(Mutex::new(zip::ZipWriter::new(file)));

        let image_format = self.image_format;
        let compression_method = self.compression_method;
        let images = images
            .into_iter()
            .map(|bytes| bytes.as_ref().to_vec())
            .collect::<Vec<_>>();

        self.progress
            .build_with_message(images.len(), "Writing the zip...")?
            .wrap_stream(futures::stream::iter(images))
            .enumerate()
            .map(|pair| {
                let zip = zip.clone();
                let options = FileOptions::<ExtendedFileOptions>::default()
                    .compression_method(compression_method);
                tokio::spawn(async move {
                    let (i, bytes) = pair;
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

    /// Save images as a zip file.
    async fn write_images<P: AsRef<Path>>(&self, images: Vec<DynamicImage>, path: P) -> Result<()> {
        let file = std::fs::File::create(path.as_ref())?;
        let zip = Arc::new(Mutex::new(zip::ZipWriter::new(file)));
        let image_format = self.image_format;
        let compression_method = self.compression_method;

        self.progress
            .build_with_message(images.len(), "Writing the zip...")?
            .wrap_stream(futures::stream::iter(images))
            .enumerate()
            .map(|(i, image)| {
                tokio::task::spawn_blocking(move || {
                    let bytes = utils::encode_image(&image, image_format)?;
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
