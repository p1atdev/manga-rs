use anyhow::{Result, ensure};

use crate::{solver::ImageSolver, utils::Bytes};

#[derive(Debug, Clone)]
pub struct Solver {
    xor_key: [u8; 8],
}

#[doc(hidden)]
pub fn xor_encrypt(data: &mut [u8], key: &[u8]) {
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
}
