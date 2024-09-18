use aes::cipher::generic_array::GenericArray;
use aes::cipher::KeyIvInit;
use aes::Aes256Dec;
use anyhow::Result;
use cbc::Decryptor;
use cipher::{BlockDecryptMut, BlockSizeUser};
use hex::decode;
use std::sync::{Arc, Mutex};

/// decrypt AES-CBC encrypted data
pub fn decrypt_aes_cbc(buffer: &[u8], key_hex: &str, iv_hex: &str) -> Result<Vec<u8>> {
    let key_bytes = decode(key_hex)?;
    let iv_bytes = decode(iv_hex)?;

    let key = GenericArray::from_slice(&key_bytes);
    let iv = GenericArray::from_slice(&iv_bytes);
    let decrypter = Decryptor::<Aes256Dec>::new(&key, &iv);
    let decrypter = Arc::new(Mutex::new(decrypter));

    let mut buffer = buffer
        .to_vec()
        .chunks(Aes256Dec::block_size())
        .map(|chunk| GenericArray::clone_from_slice(chunk))
        .collect::<Vec<GenericArray<_, _>>>();

    buffer.iter_mut().for_each(|chunk| {
        decrypter.lock().unwrap().decrypt_block_mut(chunk);
    });

    Ok(buffer.concat())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_decrypt_image() {
        let key = "2e009856520e10917accae78097a2e13d9dd7a97d3a5ea293527ec9d0132bba3";
        let iv = "e8c7e042d6ba9fb85c128d5ceb64b82f";

        let image_path = "./playground/assets/fuz-encrypted.jpeg";
        let output_path = "./playground/assets/fuz-decrypted.jpeg";

        let encrypted_data = fs::read(image_path).expect("Failed to read the encrypted image file");
        let decrypted_data = decrypt_aes_cbc(&encrypted_data, key, iv).unwrap();

        fs::write(output_path, &decrypted_data).expect("Failed to write the decrypted image file");
    }
}
