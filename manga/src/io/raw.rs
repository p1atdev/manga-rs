use std::path::{Path, PathBuf};

use anyhow::{Ok, Result};

use super::EpisodeWriter;

#[derive(Debug, Clone)]
pub struct RawWriter {
    image_format: image::ImageFormat,
    save_path: PathBuf,
}

impl RawWriter {
    pub fn new<P: AsRef<Path>>(image_format: image::ImageFormat, save_path: &P) -> Self {
        RawWriter {
            image_format,
            save_path: save_path.as_ref().to_path_buf(),
        }
    }

    pub fn default<P: AsRef<Path>>(save_path: &P) -> Self {
        RawWriter {
            image_format: image::ImageFormat::Png,
            save_path: save_path.as_ref().to_path_buf(),
        }
    }
}

impl EpisodeWriter for RawWriter {
    fn save_path(&self) -> PathBuf {
        self.save_path.clone()
    }

    async fn prepare(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.save_path).await?;

        Ok(())
    }

    async fn write_page(&self, page: usize, image: image::DynamicImage) -> Result<()> {
        let image_name = format!("{}.{}", page, self.image_format.extensions_str()[0]);
        let save_path = self.save_path().join(image_name);
        let image_format = self.image_format;

        tokio::task::spawn_blocking(move || {
            let mut file = std::fs::File::options()
                .create(true)
                .write(true)
                .truncate(true)
                .open(save_path)?;
            image.write_to(&mut file, image_format)?;
            // image.save(save_path)?;
            Ok(())
        })
        .await?
    }
}
