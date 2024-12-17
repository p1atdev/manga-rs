use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use image::{buffer, DynamicImage, ImageBuffer, Rgb};
use rayon::iter::{ParallelBridge, ParallelIterator};

use super::data::{Crop, Scramble};
use crate::{solver::ImageSolver, utils::Bytes};

#[derive(Debug, Clone)]
pub struct Solver {
    width: u32,
    height: u32,
    crops: Vec<Crop>,
}

impl Solver {
    pub fn new(width: u32, height: u32, crops: Vec<Crop>) -> Self {
        Solver {
            width,
            height,
            crops,
        }
    }
}

impl From<Scramble> for Solver {
    fn from(scramble: Scramble) -> Self {
        Solver::new(scramble.width, scramble.height, scramble.crops)
    }
}

impl Solver {
    fn move_region(
        &self,
        canvas: &Arc<Mutex<ImageBuffer<Rgb<u8>, Vec<u8>>>>,
        source: &ImageBuffer<Rgb<u8>, Vec<u8>>,
        source_tl: (u32, u32), // source top left (x, y)
        target_tl: (u32, u32), // target top left (x, y)
        width: u32,
        height: u32,
    ) {
        // move source to canvas
        (0..width)
            .flat_map(|x| {
                (0..height).map(move |y| {
                    return (x, y);
                })
            })
            .into_iter()
            .par_bridge()
            .for_each(|(x, y)| {
                let source_x = source_tl.0 + x;
                let source_y = source_tl.1 + y;
                let target_x = target_tl.0 + x;
                let target_y = target_tl.1 + y;
                let source_pixel = source.get_pixel(source_x, source_y);

                canvas
                    .lock()
                    .unwrap()
                    .put_pixel(target_x, target_y, source_pixel.clone());
            });
    }

    fn solve_buffer(
        &self,
        buffer: ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    ) -> Result<image::ImageBuffer<image::Rgb<u8>, Vec<u8>>> {
        let canvas = Arc::new(Mutex::new(
            image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::new(self.width, self.height),
        ));

        self.crops.iter().par_bridge().for_each(|crop| {
            self.move_region(
                &canvas,
                &buffer,
                (crop.scrambled_x, crop.scrambled_y),
                (crop.original_x, crop.original_y),
                crop.width,
                crop.height,
            )
        });

        let canvas = Arc::try_unwrap(canvas).unwrap().into_inner().unwrap();
        Ok(canvas)
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
        let solver = Solver::new(
            1190,
            1684,
            vec![
                Crop {
                    original_x: 596,
                    original_y: 0,
                    scrambled_x: 0,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 0,
                    original_y: 0,
                    scrambled_x: 298,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 298,
                    original_y: 842,
                    scrambled_x: 596,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 298,
                    original_y: 0,
                    scrambled_x: 894,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 0,
                    original_y: 842,
                    scrambled_x: 1192,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 0,
                    original_y: 421,
                    scrambled_x: 1490,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 894,
                    original_y: 1263,
                    scrambled_x: 1788,
                    scrambled_y: 0,
                    width: 296,
                    height: 421,
                },
                Crop {
                    original_x: 298,
                    original_y: 1263,
                    scrambled_x: 2084,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 894,
                    original_y: 0,
                    scrambled_x: 2382,
                    scrambled_y: 0,
                    width: 296,
                    height: 421,
                },
                Crop {
                    original_x: 894,
                    original_y: 842,
                    scrambled_x: 2678,
                    scrambled_y: 0,
                    width: 296,
                    height: 421,
                },
                Crop {
                    original_x: 298,
                    original_y: 421,
                    scrambled_x: 2974,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 0,
                    original_y: 1263,
                    scrambled_x: 3272,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 596,
                    original_y: 1263,
                    scrambled_x: 3570,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 596,
                    original_y: 421,
                    scrambled_x: 3868,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
                Crop {
                    original_x: 894,
                    original_y: 421,
                    scrambled_x: 4166,
                    scrambled_y: 0,
                    width: 296,
                    height: 421,
                },
                Crop {
                    original_x: 596,
                    original_y: 842,
                    scrambled_x: 4462,
                    scrambled_y: 0,
                    width: 298,
                    height: 421,
                },
            ],
        );
        let img = image::ImageReader::open("./tests/assets/mangaz-scrambled2.jpg")?.decode()?;

        let solved = solver.solve_image(img)?;
        solved.save("./tests/output/mangaz-solved.jpg")?;

        Ok(())
    }
}
