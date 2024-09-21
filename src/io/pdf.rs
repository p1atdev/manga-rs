use anyhow::Result;
use pdf_writer::{Pdf, Ref};

use crate::progress::ProgressConfig;

use super::EpisodeWriter;

/// Save as a zip file.
#[derive(Debug, Clone)]
pub struct PdfWriter {
    num_threads: usize,
    progress: ProgressConfig,
}

impl PdfWriter {
    pub fn default() -> Self {
        PdfWriter {
            num_threads: num_cpus::get(),
            progress: ProgressConfig::default(),
        }
    }
}

impl EpisodeWriter for PdfWriter {
    async fn write<P: AsRef<std::path::Path>>(
        &self,
        images: Vec<image::DynamicImage>,
        path: P,
    ) -> Result<()> {
        let mut pdf = Pdf::new();

        for (i, img) in images.iter().enumerate() {
            let page_id = Ref::new(i as i32 + 1);
        }

        Ok(())
    }
}
