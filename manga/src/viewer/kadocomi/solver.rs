use anyhow::Result;
use image::{DynamicImage, ImageBuffer, Rgb, RgbaImage};

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
    let _bytes = hex::decode(hex)?;
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&_bytes[..8]);
    Ok(bytes)
}

impl Solver {
    fn new(xor_key: &[u8; 8]) -> Self {
        Solver {
            xor_key: xor_key.clone(),
        }
    }

    pub fn from_hex(xor_key: &str) -> Result<Self> {
        let key_bytes = hex_to_bytes(xor_key)?;
        Ok(Solver { xor_key: key_bytes })
    }

    fn solve_buffer(&self, mut buffer: Vec<u8>) -> Vec<u8> {
        xor_encrypt(&mut buffer, &self.xor_key);
        buffer
    }
}

impl ImageSolver for Solver {
    fn solve<T: AsRef<[u8]>>(&self, bytes: T) -> Result<Bytes> {
        let buffer = self.solve_buffer(bytes.as_ref().to_vec());

        Ok(buffer)
    }

    fn solve_from_bytes<B: AsRef<[u8]>>(&self, bytes: B) -> Result<DynamicImage> {
        let buffer = self.solve_buffer(bytes.as_ref().to_vec());
        let solved_image = image::load_from_memory(&buffer)?;

        Ok(solved_image)
    }
}

#[cfg(test)]
mod test {
    use std::{
        fs::File,
        io::{Read, Write},
    };

    use super::*;

    #[test]
    fn test_new_solver() -> Result<()> {
        let key = "0123456789abcdef";
        let _solver = Solver::from_hex(key);

        Ok(())
    }

    #[test]
    fn test_xor_encrypt_decrypt() -> Result<()> {
        let key = hex_to_bytes("0123456789abcdef")?;
        let image_path = "./tests/assets/kadocomi-decrypted.webp";
        let encrypt_path = "./tests/output/kadocomi-encrypted.webp";

        let mut buffer = File::open(image_path)?
            .bytes()
            .collect::<Result<Vec<_>, _>>()?;

        xor_encrypt(&mut buffer, &key);

        // write
        let mut output = File::create(encrypt_path)?;
        output.write_all(&buffer)?;

        // read and decrypt
        let mut buffer = File::open(encrypt_path)?
            .bytes()
            .collect::<Result<Vec<_>, _>>()?;
        xor_encrypt(&mut buffer, &key);

        Ok(())
    }

    #[test]
    fn test_solve_image() -> Result<()> {
        let solver = Solver::from_hex("0123456789abcdef")?;
        let image_path = "./tests/assets/kadocomi-encrypted.webp";
        let buffer = File::open(image_path)?
            .bytes()
            .collect::<Result<Vec<_>, _>>()?;

        let solved = solver.solve_from_bytes(buffer)?;
        solved.save("./tests/output/kadocomi-solved.webp")?;

        // answer
        let answer = image::open("./tests/assets/kadocomi-decrypted.webp")?;
        let solved = image::open("./tests/output/kadocomi-solved.webp")?;

        assert_eq!(answer.to_rgba8(), solved.to_rgba8());

        Ok(())
    }
}
