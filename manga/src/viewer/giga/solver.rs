use anyhow::Result;
use image::{DynamicImage, ImageBuffer, Rgb};

use crate::{solver::ImageSolver, utils::Bytes};

const NUM_CELLS: u8 = 4;
const DIVISIBLE_WITH: u8 = 8;

#[derive(Debug, Clone)]
pub struct Solver {
    num_cells: u32,
    divisible_with: u32,
}

impl Solver {
    pub fn new() -> Self {
        Solver {
            num_cells: u32::from(NUM_CELLS),
            divisible_with: u32::from(DIVISIBLE_WITH),
        }
    }
}

impl Solver {
    /// transforms tiles like below:
    /// ```md
    /// \ABC D     \eim D
    /// e\fg h  -> A\ij h
    /// ij\k l     Be\o l
    /// mno\ p     Cfi\ p
    /// qrst u     qrst u
    ///
    /// ◣◹ -> ◺◥
    /// ```
    ///
    /// See playground/assets/giga-original.jpg and giga-swapped.jpg for details.
    fn swap_regions(
        &self,
        img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
        // source_tl: source top left (x, y)
        source_tl: (u32, u32),
        // target_tl: target top left (x, y)
        target_tl: (u32, u32),
        cell_width: u32,
        cell_height: u32,
    ) {
        let (source_x, source_y) = source_tl;
        let (target_x, target_y) = target_tl;

        (0..cell_width)
            .flat_map(move |x| (0..cell_height).map(move |y| (x, y)))
            .for_each(|(x, y)| {
                let source_pixel = img.get_pixel(source_x + x, source_y + y).clone();
                let target_pixel = img.get_pixel(target_x + x, target_y + y);

                img.put_pixel(source_x + x, source_y + y, *target_pixel);
                img.put_pixel(target_x + x, target_y + y, source_pixel);
            });
    }

    fn solve_buffer(
        &self,
        buffer: image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    ) -> Result<image::ImageBuffer<image::Rgb<u8>, Vec<u8>>> {
        let (width, height) = buffer.dimensions();

        let cell_width = width / (self.num_cells * self.divisible_with) * self.divisible_with;
        let cell_height = height / (self.num_cells * self.divisible_with) * self.divisible_with;

        let mut img = buffer.clone();

        let indices = (0..self.num_cells)
            .flat_map(move |i| (i..self.num_cells).map(move |j| (i, j)))
            .collect::<Vec<_>>();

        indices.iter().for_each(|&(i, j)| {
            let source = (i * cell_width, j * cell_height);
            let target = (j * cell_width, i * cell_height);

            self.swap_regions(&mut img, source, target, cell_width, cell_height);
        });

        Ok(img)
    }

    fn solve_image(&self, image: image::DynamicImage) -> Result<image::DynamicImage> {
        let buffer = image.to_rgb8();
        let solved_buffer = self.solve_buffer(buffer)?;

        Ok(image::DynamicImage::ImageRgb8(solved_buffer))
    }
}

impl ImageSolver for Solver {
    fn solve<T: AsRef<[u8]>>(&self, bytes: T) -> Result<Bytes> {
        let image = image::load_from_memory(bytes.as_ref())?;
        let solved_image = self.solve_image(image)?;

        Ok(solved_image.as_bytes().into())
    }

    fn solve_from_bytes<B: AsRef<[u8]>>(&self, bytes: B) -> Result<DynamicImage> {
        let image = image::load_from_memory(bytes.as_ref())?;
        let solved_image = self.solve_image(image)?;

        Ok(solved_image)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_solve_sample_image() -> Result<()> {
        let solver = Solver::new();
        let img = image::ImageReader::open("./tests/assets/giga-original.jpg")?.decode()?;

        let solved = solver.solve_image(img)?;
        solved.save("./tests/output/giga-solved.jpg")?;

        Ok(())
    }
}
