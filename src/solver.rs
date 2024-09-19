use anyhow::Result;
use image::DynamicImage;

/// A trait for solving image obfuscation.
pub trait ImageSolver {
    /// Solve the obfuscated bytes.
    fn solve_from_bytes(&self, bytes: Vec<u8>) -> Result<DynamicImage>;
}
