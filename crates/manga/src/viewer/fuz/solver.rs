use anyhow::Result;
use image::DynamicImage;

use crate::{solver::ImageSolver, utils::Bytes};

use super::crypto::decrypt_aes_cbc;

/// ComicFuz image solver
#[derive(Debug, Clone)]
pub struct Solver<'a> {
    key_hex: &'a str,
    iv_hex: &'a str,
}

impl<'a> Solver<'a> {
    pub fn new(key_hex: &'a str, iv_hex: &'a str) -> Self {
        Solver { key_hex, iv_hex }
    }
}

impl Solver<'_> {
    /// decrypts the image AES-CBC encryption
    fn solve_buffer(&self, buffer: Bytes) -> Result<Bytes> {
        decrypt_aes_cbc(buffer, self.key_hex, self.iv_hex)
    }
}

impl ImageSolver for Solver<'_> {
    fn solve(&self, bytes: Bytes) -> Result<Bytes> {
        self.solve_buffer(bytes)
    }

    fn solve_from_bytes(&self, bytes: Bytes) -> Result<DynamicImage> {
        let buffer = self.solve_buffer(bytes)?;
        let image = image::load_from_memory(&buffer)?;
        drop(buffer);
        Ok(image)
    }
}
