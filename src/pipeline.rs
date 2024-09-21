use crate::{
    progress::ProgressConfig,
    viewer::{giga, ViewerType},
};

/// How to save the manga
pub enum SaveFormat {
    Raw,
    Zip,
    Pdf,
}

/// Configuration for the writer
pub struct WriterConifg {
    save_format: SaveFormat,
    image_format: image::ImageFormat,
}

/// Piepline for downloading manga
pub struct PielineConfig {
    progress: ProgressConfig,
    writer: WriterConifg,
}

impl PielineConfig {
    pub fn new(progress: ProgressConfig, writer: WriterConifg) -> Self {
        PielineConfig { progress, writer }
    }

    pub fn default() -> Self {
        PielineConfig {
            progress: ProgressConfig::default(),
            writer: WriterConifg {
                save_format: SaveFormat::Raw,
                image_format: image::ImageFormat::Png,
            },
        }
    }

    pub fn get_viewer_type(url: &Url) -> ViewerType {}
}
