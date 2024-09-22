use std::{future::Future, path::Path};

use anyhow::Result;
use image::DynamicImage;

pub mod pdf;
pub mod raw;
pub mod zip;

/// A trait for saving manga to disk.
pub trait EpisodeWriter {
    /// Save images from bytes
    fn write<P: AsRef<Path>, B: AsRef<[u8]>>(
        &self,
        images: Vec<B>,
        path: P,
    ) -> impl Future<Output = Result<()>>;

    /// Save images
    fn write_images<P: AsRef<Path>>(
        &self,
        images: Vec<DynamicImage>,
        path: P,
    ) -> impl Future<Output = Result<()>>;
}
