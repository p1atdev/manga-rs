use anyhow::{Result, ensure};
use image::DynamicImage;

use crate::{solver::ImageSolver, utils::Bytes};

#[derive(Debug, Clone)]
pub struct Solver {
    xor_key: [u8; 8],
}

fn xor_encrypt(data: &mut [u8], key: &[u8]) {
    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= key[i % key.len()];
    }
}

fn hex_to_bytes(hex: &str) -> Result<[u8; 8]> {
    ensure!(
        hex.len() == 16,
        "Kadokomi XOR key must contain exactly 8 bytes"
    );
    let mut bytes = [0u8; 8];
    hex::decode_to_slice(hex, &mut bytes)?;
    Ok(bytes)
}

impl Solver {
    pub fn from_hex(xor_key: &str) -> Result<Self> {
        let key_bytes = hex_to_bytes(xor_key)?;
        Ok(Solver { xor_key: key_bytes })
    }

    fn solve_buffer(&self, mut buffer: Bytes) -> Bytes {
        xor_encrypt(&mut buffer, &self.xor_key);
        buffer
    }
}

impl ImageSolver for Solver {
    fn solve(&self, bytes: Bytes) -> Result<Bytes> {
        Ok(self.solve_buffer(bytes))
    }

    fn solve_from_bytes(&self, bytes: Bytes) -> Result<DynamicImage> {
        let buffer = self.solve_buffer(bytes);
        let solved_image = image::load_from_memory(&buffer)?;
        drop(buffer);

        Ok(solved_image)
    }
}

#[cfg(test)]
mod test {
    use std::fs;

    use super::*;

    #[test]
    fn test_new_solver() -> Result<()> {
        let key = "0123456789abcdef";
        Solver::from_hex(key)?;
        assert!(Solver::from_hex("00").is_err());

        Ok(())
    }

    #[test]
    fn test_xor_encrypt_decrypt() -> Result<()> {
        let key = hex_to_bytes("0123456789abcdef")?;
        let original = fs::read("./tests/assets/kadocomi-decrypted.webp")?;
        let mut buffer = original.clone();

        xor_encrypt(&mut buffer, &key);
        xor_encrypt(&mut buffer, &key);

        assert_eq!(buffer, original);
        Ok(())
    }

    #[test]
    fn test_solve_image() -> Result<()> {
        let solver = Solver::from_hex("0123456789abcdef")?;
        let buffer = fs::read("./tests/assets/kadocomi-encrypted.webp")?;

        let solved = solver.solve_from_bytes(buffer)?;
        let answer = image::open("./tests/assets/kadocomi-decrypted.webp")?;

        assert_eq!(answer.to_rgba8(), solved.to_rgba8());

        Ok(())
    }

    #[test]
    fn test_solve_reuses_input_allocation() -> Result<()> {
        let solver = Solver::from_hex("0123456789abcdef")?;
        let encrypted = fs::read("./tests/assets/kadocomi-encrypted.webp")?;
        let encrypted_allocation = encrypted.as_ptr();

        let decrypted = solver.solve(encrypted)?;

        assert_eq!(decrypted.as_ptr(), encrypted_allocation);
        assert_eq!(
            decrypted,
            fs::read("./tests/assets/kadocomi-decrypted.webp")?
        );
        Ok(())
    }
}
