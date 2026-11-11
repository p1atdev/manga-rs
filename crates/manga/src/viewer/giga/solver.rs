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

impl Default for Solver {
    fn default() -> Self {
        Self::new()
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
        source_tl: (u32, u32),
        target_tl: (u32, u32),
        cell_width: u32,
        cell_height: u32,
    ) {
        let stride = img.width() as usize * 3;
        let row_len = cell_width as usize * 3;
        let bytes = img.as_mut();
        for y in 0..cell_height {
            let source = (source_tl.1 + y) as usize * stride + source_tl.0 as usize * 3;
            let target = (target_tl.1 + y) as usize * stride + target_tl.0 as usize * 3;
            let (low, high) = if source < target {
                (source, target)
            } else {
                (target, source)
            };
            // Only off-diagonal tiles reach here, so their row ranges do not overlap.
            let (before, after) = bytes.split_at_mut(high);
            before[low..low + row_len].swap_with_slice(&mut after[..row_len]);
        }
    }

    fn solve_buffer(
        &self,
        mut buffer: image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    ) -> Result<image::ImageBuffer<image::Rgb<u8>, Vec<u8>>> {
        let (width, height) = buffer.dimensions();

        let cell_width = width / (self.num_cells * self.divisible_with) * self.divisible_with;
        let cell_height = height / (self.num_cells * self.divisible_with) * self.divisible_with;

        if cell_width == 0 || cell_height == 0 {
            return Ok(buffer);
        }

        for i in 0..self.num_cells {
            for j in i + 1..self.num_cells {
                let source = (i * cell_width, j * cell_height);
                let target = (j * cell_width, i * cell_height);

                self.swap_regions(&mut buffer, source, target, cell_width, cell_height);
            }
        }

        Ok(buffer)
    }

    fn solve_image(&self, image: image::DynamicImage) -> Result<image::DynamicImage> {
        let buffer = image.into_rgb8();
        let solved_buffer = self.solve_buffer(buffer)?;

        Ok(image::DynamicImage::ImageRgb8(solved_buffer))
    }
}

impl ImageSolver for Solver {
    fn solve(&self, bytes: Bytes) -> Result<Bytes> {
        let image = image::load_from_memory(&bytes)?;
        drop(bytes);
        let solved_image = self.solve_image(image)?;

        Ok(solved_image.into_bytes())
    }

    fn solve_from_bytes(&self, bytes: Bytes) -> Result<DynamicImage> {
        let image = image::load_from_memory(&bytes)?;
        drop(bytes);
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
        let original = image::ImageReader::open("./tests/assets/giga-original.jpg")?.decode()?;

        let buffer = original.clone().into_rgb8();
        let buffer_allocation = buffer.as_ptr();
        let solved_buffer = solver.solve_buffer(buffer)?;
        assert_eq!(solved_buffer.as_ptr(), buffer_allocation);

        let solved = solver.solve_image(original.clone())?;
        let restored = solver.solve_image(solved.clone())?;

        assert_ne!(solved.to_rgb8(), original.to_rgb8());
        assert_eq!(restored.to_rgb8(), original.to_rgb8());
        Ok(())
    }
    // Snapshot of the pre-optimization implementation for paired benchmarks.
    struct Baseline {
        num_cells: u32,
        divisible_with: u32,
    }
    impl Baseline {
        fn new() -> Self {
            Self {
                num_cells: 4,
                divisible_with: 8,
            }
        }
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
                    let source_pixel = *img.get_pixel(source_x + x, source_y + y);
                    let target_pixel = img.get_pixel(target_x + x, target_y + y);

                    img.put_pixel(source_x + x, source_y + y, *target_pixel);
                    img.put_pixel(target_x + x, target_y + y, source_pixel);
                });
        }

        fn solve_buffer(
            &self,
            mut buffer: image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
        ) -> Result<image::ImageBuffer<image::Rgb<u8>, Vec<u8>>> {
            let (width, height) = buffer.dimensions();

            let cell_width = width / (self.num_cells * self.divisible_with) * self.divisible_with;
            let cell_height = height / (self.num_cells * self.divisible_with) * self.divisible_with;

            for i in 0..self.num_cells {
                for j in i..self.num_cells {
                    let source = (i * cell_width, j * cell_height);
                    let target = (j * cell_width, i * cell_height);

                    self.swap_regions(&mut buffer, source, target, cell_width, cell_height);
                }
            }

            Ok(buffer)
        }
    }

    fn reference(mut image: ImageBuffer<Rgb<u8>, Vec<u8>>) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        let width = image.width() / 32 * 8;
        let height = image.height() / 32 * 8;
        for i in 0..4 {
            for j in i..4 {
                for x in 0..width {
                    for y in 0..height {
                        let a = *image.get_pixel(i * width + x, j * height + y);
                        let b = *image.get_pixel(j * width + x, i * height + y);
                        image.put_pixel(i * width + x, j * height + y, b);
                        image.put_pixel(j * width + x, i * height + y, a);
                    }
                }
            }
        }
        image
    }

    #[test]
    fn matches_reference_for_small_remainders_and_transparent_input() {
        for (width, height) in [
            (0, 4),
            (4, 0),
            (1, 1),
            (31, 33),
            (32, 32),
            (33, 63),
            (63, 64),
            (64, 65),
            (65, 33),
            (101, 103),
            (1201, 1803),
        ] {
            let rgba = image::RgbaImage::from_fn(width, height, |x, y| {
                image::Rgba([
                    (x.wrapping_mul(13)) as u8,
                    (y.wrapping_mul(17)) as u8,
                    (x ^ y) as u8,
                    ((x + y) % 256) as u8,
                ])
            });
            let rgb = DynamicImage::ImageRgba8(rgba).into_rgb8();
            assert_eq!(
                Solver::new().solve_buffer(rgb.clone()).unwrap(),
                reference(rgb),
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn public_jpeg_matches_independent_baseline_at_every_pixel() {
        let jpeg = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/giga-tonari-yj-public-20261111.jpg"
        ));
        let decoded = image::load_from_memory(jpeg).unwrap().into_rgb8();
        let expected = Baseline::new().solve_buffer(decoded.clone()).unwrap();
        let actual = Solver::new().solve_buffer(decoded).unwrap();
        assert_eq!(actual.dimensions(), expected.dimensions());
        assert!(
            actual.as_raw() == expected.as_raw(),
            "public JPEG restored pixels differ from the baseline"
        );
        let api = Solver::new()
            .solve_from_bytes(jpeg.to_vec())
            .unwrap()
            .into_rgb8();
        assert_eq!(api.dimensions(), expected.dimensions());
        assert!(
            api.as_raw() == expected.as_raw(),
            "public JPEG byte API pixels differ from the baseline"
        );
    }

    #[test]
    #[ignore = "manual paired release benchmark on the public JPEG fixture"]
    fn benchmark_giga_public_jpeg_paired_abba() {
        use std::hint::black_box;
        let jpeg = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/giga-tonari-yj-public-20261111.jpg"
        ));
        let input = image::load_from_memory(jpeg).unwrap().into_rgb8();
        let old = Baseline::new();
        let new = Solver::new();
        let expected = old.solve_buffer(input.clone()).unwrap();
        let actual = new.solve_buffer(input.clone()).unwrap();
        assert!(
            actual.as_raw() == expected.as_raw(),
            "public JPEG pixels differ"
        );
        crate::benchmark::paired(
            "GIGA_PUBLIC_JPEG_BUFFER 800x1137",
            || {
                black_box(old.solve_buffer(black_box(input.clone())).unwrap());
            },
            || {
                black_box(new.solve_buffer(black_box(input.clone())).unwrap());
            },
        );
        let baseline = || {
            let data = black_box(jpeg.to_vec());
            let image = image::load_from_memory(&data).unwrap();
            drop(data);
            DynamicImage::ImageRgb8(old.solve_buffer(image.into_rgb8()).unwrap())
        };
        let actual = new.solve_from_bytes(jpeg.to_vec()).unwrap().into_rgb8();
        assert!(
            actual.as_raw() == expected.as_raw(),
            "public JPEG byte API pixels differ"
        );
        crate::benchmark::paired(
            "GIGA_PUBLIC_JPEG_DECODE 800x1137",
            || {
                black_box(baseline());
            },
            || {
                black_box(new.solve_from_bytes(black_box(jpeg.to_vec())).unwrap());
            },
        );
    }

    #[test]
    #[ignore = "manual paired release benchmark; each interval lasts at least 50ms"]
    fn benchmark_giga_paired_abba() {
        use std::{hint::black_box, io::Cursor};
        let old = Baseline::new();
        let new = Solver::new();
        for (width, height) in [(32, 32), (1200, 1800), (1603, 2567)] {
            let input = ImageBuffer::from_fn(width, height, |x, y| {
                Rgb([(x % 251) as u8, (y % 251) as u8, ((x ^ y) % 251) as u8])
            });
            assert_eq!(
                old.solve_buffer(input.clone()).unwrap(),
                new.solve_buffer(input.clone()).unwrap()
            );
            crate::benchmark::paired(
                &format!("GIGA_BUFFER {width}x{height}"),
                || {
                    black_box(old.solve_buffer(black_box(input.clone())).unwrap());
                },
                || {
                    black_box(new.solve_buffer(black_box(input.clone())).unwrap());
                },
            );
        }
        let input = ImageBuffer::from_fn(1200, 1800, |x, y| {
            Rgb([(x % 251) as u8, (y % 251) as u8, ((x ^ y) % 251) as u8])
        });
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(input)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = encoded.into_inner();
        let baseline = || {
            let data = black_box(bytes.clone());
            let image = image::load_from_memory(&data).unwrap();
            drop(data);
            DynamicImage::ImageRgb8(old.solve_buffer(image.into_rgb8()).unwrap())
        };
        assert_eq!(
            baseline().to_rgb8(),
            new.solve_from_bytes(bytes.clone()).unwrap().to_rgb8()
        );
        crate::benchmark::paired(
            "GIGA_PNG 1200x1800",
            || {
                black_box(baseline());
            },
            || {
                black_box(new.solve_from_bytes(black_box(bytes.clone())).unwrap());
            },
        );
    }
    #[test]
    #[ignore = "manual Time Profiler workload"]
    fn profile_giga() {
        use std::{
            hint::black_box,
            time::{Duration, Instant},
        };
        let image = ImageBuffer::from_fn(1200, 1800, |x, y| {
            Rgb([(x % 251) as u8, (y % 251) as u8, ((x ^ y) % 251) as u8])
        });
        let old = Baseline::new();
        let new = Solver::new();
        let mode = std::env::var("SOLVER_PROFILE").unwrap_or_else(|_| "new".into());
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(15) {
            let input = black_box(image.clone());
            let result = if mode == "old" {
                old.solve_buffer(input).unwrap()
            } else {
                new.solve_buffer(input).unwrap()
            };
            black_box(result);
        }
    }
}
