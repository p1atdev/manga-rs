use anyhow::Result;
use image::DynamicImage;

use crate::utils::Bytes;

/// A trait for solving image obfuscation.
pub trait ImageSolver {
    /// Solve the obfuscated bytes.
    fn solve(&self, bytes: Bytes) -> Result<Bytes>;
    /// Solve the obfuscated bytes to an image.
    fn solve_from_bytes(&self, bytes: Bytes) -> Result<DynamicImage>;
}

/// An empty solver that does nothing.
pub struct EmptySolver;

impl ImageSolver for EmptySolver {
    fn solve(&self, bytes: Bytes) -> Result<Bytes> {
        Ok(bytes)
    }

    fn solve_from_bytes(&self, bytes: Bytes) -> Result<DynamicImage> {
        let image = image::load_from_memory(&bytes)?;
        drop(bytes);
        Ok(image)
    }
}
