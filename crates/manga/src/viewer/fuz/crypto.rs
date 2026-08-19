use aes::Aes256Dec;
use aes::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
use anyhow::Result;
use cbc::Decryptor;

/// decrypt AES-CBC encrypted data
pub fn decrypt_aes_cbc(mut buffer: Vec<u8>, key_hex: &str, iv_hex: &str) -> Result<Vec<u8>> {
    let mut key_bytes = [0; 32];
    let mut iv_bytes = [0; 16];
    hex::decode_to_slice(key_hex, &mut key_bytes)?;
    hex::decode_to_slice(iv_hex, &mut iv_bytes)?;

    let decrypter = Decryptor::<Aes256Dec>::new(&key_bytes.into(), &iv_bytes.into());
    let decrypted_len = decrypter
        .decrypt_padded::<Pkcs7>(&mut buffer)
        .map_err(|_| anyhow::anyhow!("invalid AES-CBC padding"))?
        .len();
    buffer.truncate(decrypted_len);

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use std::fs;

    #[test]
    fn test_decrypt_image() {
        let key = "2e009856520e10917accae78097a2e13d9dd7a97d3a5ea293527ec9d0132bba3";
        let iv = "e8c7e042d6ba9fb85c128d5ceb64b82f";

        let image_path = "./tests/assets/fuz-encrypted.jpeg";
        let encrypted_data = fs::read(image_path).expect("Failed to read the encrypted image file");
        let encrypted_allocation = encrypted_data.as_ptr();
        let decrypted_data = decrypt_aes_cbc(encrypted_data, key, iv).unwrap();
        assert_eq!(decrypted_data.as_ptr(), encrypted_allocation);
        assert!(decrypted_data.ends_with(&[0xff, 0xd9]));
        let image = image::load_from_memory(&decrypted_data).unwrap();
        assert_ne!(image.dimensions(), (0, 0));
    }
}
