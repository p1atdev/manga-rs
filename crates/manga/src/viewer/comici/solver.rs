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
        let tile_width = source.width() / GRID_SIZE as u32;
        let tile_height = source.height() / GRID_SIZE as u32;
        if tile_width == 0 || tile_height == 0 {
            return source;
        }
        let mut canvas = source.clone();

        for (target_index, source_index) in self.scramble.tile_sources().iter().copied().enumerate()
        {
            let source_x = source_index as u32 / GRID_SIZE as u32 * tile_width;
            let source_y = source_index as u32 % GRID_SIZE as u32 * tile_height;
            let target_x = target_index as u32 / GRID_SIZE as u32 * tile_width;
            let target_y = target_index as u32 % GRID_SIZE as u32 * tile_height;

            let stride = source.width() as usize * 4;
            let row_len = tile_width as usize * 4;
            let source_bytes = source.as_raw();
            let target_bytes = canvas.as_mut();
            for y in 0..tile_height {
                let from = (source_y + y) as usize * stride + source_x as usize * 4;
                let to = (target_y + y) as usize * stride + target_x as usize * 4;
                target_bytes[to..to + row_len].copy_from_slice(&source_bytes[from..from + row_len]);
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

    // Snapshot of the pre-optimization implementation for paired benchmarks.
    struct Baseline {
        scramble: Scramble,
    }
    impl Baseline {
        fn solve_buffer(
            &self,
            source: ImageBuffer<Rgba<u8>, Vec<u8>>,
        ) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
            let mut canvas = source.clone();
            let tile_width = source.width() / GRID_SIZE as u32;
            let tile_height = source.height() / GRID_SIZE as u32;

            for (target_index, source_index) in
                self.scramble.tile_sources().iter().copied().enumerate()
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
    }

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
    #[test]
    fn matches_reference_for_small_remainders_and_transparency() {
        for (width, height) in [
            (0, 4),
            (4, 0),
            (1, 1),
            (3, 5),
            (4, 4),
            (5, 7),
            (7, 8),
            (8, 9),
            (9, 3),
            (101, 103),
            (1201, 1803),
        ] {
            let source = ImageBuffer::from_fn(width, height, |x, y| {
                Rgba([
                    (x.wrapping_mul(13)) as u8,
                    (y.wrapping_mul(17)) as u8,
                    (x ^ y) as u8,
                    ((x + y) % 256) as u8,
                ])
            });
            let spec = scramble();
            let mut expected = source.clone();
            for (dest, src) in spec.tile_sources().iter().copied().enumerate() {
                copy_tile(&mut expected, &source, src, dest);
            }
            assert_eq!(
                Solver::new(spec).solve_buffer(source),
                expected,
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn zero_width_with_extreme_height_returns_an_empty_image() {
        let source = ImageBuffer::from_raw(0, u32::MAX, Vec::<u8>::new()).unwrap();
        let result = Solver::new(scramble()).solve_buffer(source);
        assert_eq!(result.dimensions(), (0, u32::MAX));
        assert!(result.as_raw().is_empty());
    }

    #[test]
    fn matches_reference_for_three_and_sixteen_tile_cycles() {
        let source = ImageBuffer::from_fn(65, 67, |x, y| {
            Rgba([
                x as u8,
                y as u8,
                (x ^ y) as u8,
                ((x * 7 + y * 11) % 256) as u8,
            ])
        });
        for permutation in [
            "[1,2,0,3,4,5,6,7,8,9,10,11,12,13,14,15]",
            "[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,0]",
        ] {
            let spec: Scramble =
                serde_json::from_value(serde_json::Value::String(permutation.to_owned())).unwrap();
            let mut expected = source.clone();
            for (dest, src) in spec.tile_sources().iter().copied().enumerate() {
                copy_tile(&mut expected, &source, src, dest);
            }
            assert_eq!(Solver::new(spec).solve_buffer(source.clone()), expected);
        }
    }

    #[test]
    #[ignore = "manual paired release benchmark; each interval lasts at least 50ms"]
    fn benchmark_comici_paired_abba() {
        use std::{hint::black_box, io::Cursor};
        let old = Baseline {
            scramble: scramble(),
        };
        let new = Solver::new(scramble());
        for (width, height) in [(32, 32), (1200, 1800), (1603, 2567)] {
            let input = ImageBuffer::from_fn(width, height, |x, y| {
                Rgba([
                    (x % 251) as u8,
                    (y % 251) as u8,
                    ((x ^ y) % 251) as u8,
                    ((x + y) % 256) as u8,
                ])
            });
            assert_eq!(
                old.solve_buffer(input.clone()),
                new.solve_buffer(input.clone())
            );
            crate::benchmark::paired(
                &format!("COMICI_BUFFER {width}x{height}"),
                || {
                    black_box(old.solve_buffer(black_box(input.clone())));
                },
                || {
                    black_box(new.solve_buffer(black_box(input.clone())));
                },
            );
        }
        let input = ImageBuffer::from_fn(1200, 1800, |x, y| {
            Rgba([
                (x % 251) as u8,
                (y % 251) as u8,
                ((x ^ y) % 251) as u8,
                ((x + y) % 256) as u8,
            ])
        });
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(input)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = encoded.into_inner();
        let baseline = || {
            let data = black_box(bytes.clone());
            let image = image::load_from_memory(&data).unwrap();
            drop(data);
            DynamicImage::ImageRgba8(old.solve_buffer(image.into_rgba8()))
        };
        assert_eq!(
            baseline().to_rgba8(),
            new.solve_from_bytes(bytes.clone()).unwrap().to_rgba8()
        );
        crate::benchmark::paired(
            "COMICI_PNG 1200x1800",
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
    fn profile_comici() {
        use std::{
            hint::black_box,
            time::{Duration, Instant},
        };
        let image = ImageBuffer::from_fn(1200, 1800, |x, y| {
            Rgba([
                (x % 251) as u8,
                (y % 251) as u8,
                ((x ^ y) % 251) as u8,
                ((x + y) % 256) as u8,
            ])
        });
        let old = Baseline {
            scramble: scramble(),
        };
        let new = Solver::new(scramble());
        let mode = std::env::var("SOLVER_PROFILE").unwrap_or_else(|_| "new".into());
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(15) {
            let input = black_box(image.clone());
            let result = if mode == "old" {
                old.solve_buffer(input)
            } else {
                new.solve_buffer(input)
            };
            black_box(result);
        }
    }
}
