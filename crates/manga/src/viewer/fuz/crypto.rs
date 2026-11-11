pub use manga_image::viewer::fuz::crypto::decrypt_aes_cbc;

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
