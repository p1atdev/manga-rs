use anyhow::Result;
use image::{DynamicImage, ImageBuffer, Rgb, Rgba};

/// A trait for solving image obfuscation.
pub trait ImageSolver {
    /// Solve the obfuscated image buffer.
    fn solve_buffer(
        &self,
        buffer: ImageBuffer<Rgb<u8>, Vec<u8>>,
    ) -> Result<ImageBuffer<Rgb<u8>, Vec<u8>>>;

    /// Solve the obfuscated image.
    fn solve_image(&self, image: DynamicImage) -> Result<DynamicImage>;

    /// Solve the obfuscated bytes.
    fn solve_from_bytes(&self, bytes: Vec<u8>) -> Result<DynamicImage>;
}
