// Seedrandom and tile grouping adapted from eggplants/getpiccoma (MIT).
// See licenses/getpiccoma-MIT.txt.
use anyhow::{Result, ensure};
struct Arc4 {
    state: [u8; 256],
    i: usize,
    j: usize,
}
impl Arc4 {
    fn new(seed: &str) -> Result<Self> {
        ensure!(
            !seed.is_empty() && seed.is_ascii() && seed.len() <= 256,
            "invalid shuffle seed"
        );
        let mut state = [0u8; 256];
        for (i, item) in state.iter_mut().enumerate() {
            *item = i as u8;
        }
        let key = seed.as_bytes();
        let mut j = 0usize;
        for i in 0..256 {
            j = (j + state[i] as usize + key[i % key.len()] as usize) & 255;
            state.swap(i, j);
        }
        let mut arc = Self { state, i: 0, j: 0 };
        for _ in 0..256 {
            arc.byte();
        }
        Ok(arc)
    }
    fn byte(&mut self) -> u8 {
        self.i = (self.i + 1) & 255;
        self.j = (self.j + self.state[self.i] as usize) & 255;
        self.state.swap(self.i, self.j);
        self.state[(self.state[self.i] as usize + self.state[self.j] as usize) & 255]
    }
    fn random(&mut self) -> f64 {
        let mut n = 0f64;
        for _ in 0..6 {
            n = n * 256.0 + f64::from(self.byte());
        }
        let mut denominator = 281_474_976_710_656f64;
        let mut extra = 0u8;
        while n < 4_503_599_627_370_496f64 {
            n = (n + f64::from(extra)) * 256.0;
            denominator *= 256.0;
            extra = self.byte();
        }
        while n >= 9_007_199_254_740_992f64 {
            n /= 2.0;
            denominator /= 2.0;
            extra >>= 1;
        }
        (n + f64::from(extra)) / denominator
    }
}

pub fn shuffle_order(size: usize, seed: &str) -> Result<Vec<usize>> {
    let mut random = Arc4::new(seed)?;
    let mut left: Vec<usize> = (0..size).collect();
    let mut out = Vec::with_capacity(size);
    while !left.is_empty() {
        let index = (random.random() * left.len() as f64).floor() as usize;
        ensure!(index < left.len(), "invalid shuffle index");
        out.push(left.remove(index));
    }
    Ok(out)
}

pub fn restore_image(image: image::RgbaImage, seed: &str) -> Result<image::RgbaImage> {
    use image::GenericImage;
    let (width, height) = image.dimensions();
    let cols = width.div_ceil(50);
    let rows = height.div_ceil(50);
    type TileGroup = ((u32, u32), Vec<(u32, u32)>);
    let mut groups: Vec<TileGroup> = Vec::new();
    for y in 0..rows {
        for x in 0..cols {
            let px = x * 50;
            let py = y * 50;
            let shape = (50.min(width - px), 50.min(height - py));
            if let Some((_, items)) = groups.iter_mut().find(|(size, _)| *size == shape) {
                items.push((px, py));
            } else {
                groups.push((shape, vec![(px, py)]));
            }
        }
    }
    let mut out = image.clone();
    for ((w, h), tiles) in groups {
        let order = shuffle_order(tiles.len(), seed)?;
        for (dest, src) in tiles.iter().zip(order) {
            let (sx, sy) = tiles[src];
            let cropped = image::imageops::crop_imm(&image, sx, sy, w, h).to_image();
            out.copy_from(&cropped, dest.0, dest.1)?;
        }
    }
    Ok(out)
}
