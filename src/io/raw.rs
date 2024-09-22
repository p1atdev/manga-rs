use std::{io::Cursor, sync::Arc};

use anyhow::Result;
use futures::{future, StreamExt};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::progress::ProgressConfig;

use super::EpisodeWriter;

#[derive(Debug, Clone)]
pub struct RawWriter {
    progress: ProgressConfig,
    image_format: image::ImageFormat,
    num_threads: usize,
}

impl RawWriter {
    pub fn new(
        progress: ProgressConfig,
        image_format: image::ImageFormat,
        num_threads: usize,
    ) -> Self {
        RawWriter {
            progress,
            image_format,
            num_threads,
        }
    }

    pub fn default() -> Self {
        RawWriter {
            progress: ProgressConfig::default(),
            image_format: image::ImageFormat::Png,
            num_threads: num_cpus::get(),
        }
    }
}

impl EpisodeWriter for RawWriter {
    async fn write<P: AsRef<std::path::Path>>(
        &self,
        images: Vec<image::DynamicImage>,
        path: P,
    ) -> Result<()> {
        let image_format = self.image_format;

        tokio::fs::create_dir_all(path.as_ref()).await?;
        let path = Arc::new(path.as_ref().to_path_buf());

        self.progress
            .build_with_message(images.len(), "Writing images...")?
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
                let path = path.clone();
                tokio::spawn(async move {
                    let (i, bytes) = pair?;
                    let image_name = format!("{}.{}", i, image_format.extensions_str()[0]);

                    let mut file = File::options()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(path.join(image_name))
                        .await?;
                    file.write_all(&bytes).await?;

                    Result::<_>::Ok(())
                })
            })
            .buffer_unordered(self.num_threads)
            .collect::<Vec<_>>()
            .await;

        Ok(())
    }
}
