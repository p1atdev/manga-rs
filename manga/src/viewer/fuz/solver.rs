use anyhow::Result;
use image::DynamicImage;

use crate::{solver::ImageSolver, utils::Bytes};

use super::crypto::decrypt_aes_cbc;

/// ComicFuz image solver
#[derive(Debug, Clone)]
pub struct Solver {
    key_hex: String,
    iv_hex: String,
}

impl Solver {
    pub fn new(key_hex: &str, iv_hex: &str) -> Self {
        Solver {
            key_hex: key_hex.to_string(),
            iv_hex: iv_hex.to_string(),
        }
    }
}

impl Solver {
    /// decrypts the image AES-CBC encryption
    fn solve_buffer<B: AsRef<[u8]>>(&self, buffer: B) -> Result<Bytes> {
        decrypt_aes_cbc(buffer.as_ref(), &self.key_hex, &self.iv_hex)
    }
}

impl ImageSolver for Solver {
    fn solve<T: AsRef<[u8]>>(&self, bytes: T) -> Result<Bytes> {
        self.solve_buffer(bytes)
    }

    fn solve_from_bytes<B: AsRef<[u8]>>(&self, bytes: B) -> Result<DynamicImage> {
        let buffer = self.solve_buffer(bytes)?;
        let image = image::load_from_memory(&buffer)?;
        Ok(image)
    }
}
