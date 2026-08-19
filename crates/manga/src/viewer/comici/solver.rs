use anyhow::Result;
use image::{DynamicImage, ImageBuffer, Rgba};

use crate::{solver::ImageSolver, utils::Bytes};

use super::data::{GRID_SIZE, Scramble};

#[derive(Debug, Clone)]
pub struct Solver {
    scramble: Scramble,
}

impl Solver {
    pub fn new(scramble: Scramble) -> Self {
        Self { scramble }
    }

    fn solve_buffer(
        &self,
        source: ImageBuffer<Rgba<u8>, Vec<u8>>,
    ) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        let mut canvas = source.clone();
        let tile_width = source.width() / GRID_SIZE as u32;
        let tile_height = source.height() / GRID_SIZE as u32;

        for (target_index, source_index) in self.scramble.tile_sources().iter().copied().enumerate()
        {
            let source_x = source_index as u32 / GRID_SIZE as u32 * tile_width;
            let source_y = source_index as u32 % GRID_SIZE as u32 * tile_height;
            let target_x = target_index as u32 / GRID_SIZE as u32 * tile_width;
            let target_y = target_index as u32 % GRID_SIZE as u32 * tile_height;

            for x in 0..tile_width {
                for y in 0..tile_height {
                    canvas.put_pixel(
                        target_x + x,
                        target_y + y,
                        *source.get_pixel(source_x + x, source_y + y),
                    );
                }
            }
        }

        canvas
    }

    fn solve_image(&self, image: DynamicImage) -> DynamicImage {
        DynamicImage::ImageRgba8(self.solve_buffer(image.into_rgba8()))
    }
}

impl ImageSolver for Solver {
    fn solve(&self, bytes: Bytes) -> Result<Bytes> {
        let image = image::load_from_memory(&bytes)?;
        drop(bytes);
        Ok(self.solve_image(image).into_bytes())
    }

    fn solve_from_bytes(&self, bytes: Bytes) -> Result<DynamicImage> {
        let image = image::load_from_memory(&bytes)?;
        drop(bytes);
        Ok(self.solve_image(image))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scramble() -> Scramble {
        serde_json::from_str(r#""[13,0,7,10,1,8,12,5,15,14,2,9,11,4,6,3]""#).unwrap()
    }

    fn copy_tile(
        target: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
        source: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        source_index: usize,
        target_index: usize,
    ) {
        let tile_width = source.width() / GRID_SIZE as u32;
        let tile_height = source.height() / GRID_SIZE as u32;
        let source_x = source_index as u32 / GRID_SIZE as u32 * tile_width;
        let source_y = source_index as u32 % GRID_SIZE as u32 * tile_height;
        let target_x = target_index as u32 / GRID_SIZE as u32 * tile_width;
        let target_y = target_index as u32 % GRID_SIZE as u32 * tile_height;

        for x in 0..tile_width {
            for y in 0..tile_height {
                target.put_pixel(
                    target_x + x,
                    target_y + y,
                    *source.get_pixel(source_x + x, source_y + y),
                );
            }
        }
    }

    #[test]
    fn restores_tiles_and_keeps_remainders_untouched() {
        let original =
            ImageBuffer::from_fn(10, 9, |x, y| Rgba([x as u8, y as u8, (x + y) as u8, 255]));
        let scramble = scramble();
        let mut scrambled = original.clone();

        for (original_index, scrambled_index) in scramble.tile_sources().iter().copied().enumerate()
        {
            copy_tile(&mut scrambled, &original, original_index, scrambled_index);
        }

        let restored = Solver::new(scramble).solve_buffer(scrambled);
        assert_eq!(restored, original);
    }
}
