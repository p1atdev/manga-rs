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
