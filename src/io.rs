use std::{future::Future, path::Path};

use anyhow::Result;
use image::DynamicImage;

pub mod pdf;
pub mod zip;

/// A trait for saving manga to disk.
pub trait EpisodeWriter {
    /// Save images
    fn write<P: AsRef<Path>>(
        &self,
        images: Vec<DynamicImage>,
        path: P,
    ) -> impl Future<Output = Result<()>>;
}
