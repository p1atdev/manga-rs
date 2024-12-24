use anyhow::Result;
use image::DynamicImage;

use crate::utils::Bytes;

/// A trait for solving image obfuscation.
pub trait ImageSolver {
    /// Solve the obfuscated bytes.
    fn solve<T: AsRef<[u8]>>(&self, bytes: T) -> Result<Bytes>;
    /// Solve the obfuscated bytes to an image.
    fn solve_from_bytes<B: AsRef<[u8]>>(&self, bytes: B) -> Result<DynamicImage>;
}

/// An empty solver that does nothing.
pub struct EmptySolver;

impl ImageSolver for EmptySolver {
    fn solve<T: AsRef<[u8]>>(&self, bytes: T) -> Result<Bytes> {
        Ok(bytes.as_ref().to_vec())
    }

    fn solve_from_bytes<B: AsRef<[u8]>>(&self, bytes: B) -> Result<DynamicImage> {
        Ok(image::load_from_memory(bytes.as_ref())?)
    }
}
