use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "manga-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn decrypts_kadokomi_without_reencoding_the_image() {
    let temp = TempDir::new();
    let input =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets/kadocomi-encrypted.webp");
    let expected = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets/kadocomi-decrypted.webp"),
    )
    .unwrap();
    let key = temp.path("key");
    fs::write(&key, "0123456789abcdef\n").unwrap();
    for command in ["decrypt", "decode"] {
        let output = temp.path(&format!("{command}.webp"));
        let status = Command::new(env!("CARGO_BIN_EXE_manga"))
            .args([command, "kadokomi"])
            .arg(&input)
            .arg("--key-file")
            .arg(&key)
            .arg("--output")
            .arg(&output)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read(output).unwrap(), expected);
    }
}

#[test]
fn decrypt_preserves_valid_png_metadata_without_changing_bytes() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    let output = temp.path("output.png");
    let key = temp.path("key");
    fs::write(&key, "0000000000000000").unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
        .save(&input)
        .unwrap();
    let mut png = fs::read(&input).unwrap();
    let end = png.len() - 12;
    png.splice(
        end..end,
        [
            0, 0, 0, 3, b't', b'E', b'X', b't', b'a', 0, b'b', 0xdc, 0x49, 0xa2, 0x3b,
        ],
    );
    fs::write(&input, &png).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decrypt", "kadokomi"])
        .arg(&input)
        .arg("--key-file")
        .arg(key)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(output).unwrap(), png);
}

#[test]
fn decryption_preserves_large_png_bytes_without_default_decode_limit() {
    let temp = TempDir::new();
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/direct-synthetic-large-rgba8.png");
    let original = fs::read(&input).unwrap();
    let key = temp.path("key");
    fs::write(&key, "0000000000000000").unwrap();
    for command in ["decrypt", "decode"] {
        let output = temp.path(&format!("{command}.png"));
        let result = Command::new(env!("CARGO_BIN_EXE_manga"))
            .args([command, "kadokomi"])
            .arg(&input)
            .arg("--key-file")
            .arg(&key)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fs::read(output).unwrap(), original);
    }
}

#[test]
fn decryption_rejects_png_with_corrupt_iend_crc_without_output() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    let key = temp.path("key");
    fs::write(&key, "0000000000000000").unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
        .save(&input)
        .unwrap();
    let mut png = fs::read(&input).unwrap();
    *png.last_mut().unwrap() ^= 1;
    fs::write(&input, png).unwrap();
    for command in ["decrypt", "decode"] {
        let output = temp.path(&format!("{command}.png"));
        let result = Command::new(env!("CARGO_BIN_EXE_manga"))
            .args([command, "kadokomi"])
            .arg(&input)
            .arg("--key-file")
            .arg(&key)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            !result.status.success(),
            "{command} accepted a corrupt IEND CRC"
        );
        assert!(!output.exists());
    }
}

#[test]
fn restores_a_giga_png_without_changing_its_format() {
    let temp = TempDir::new();
    let source = image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([x as u8, y as u8, (x ^ y) as u8, 255])
    });
    let input = temp.path("input.png");
    let output = temp.path("output.png");
    source.save(&input).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "giga"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    let bytes = fs::read(output).unwrap();
    assert_eq!(
        image::guess_format(&bytes).unwrap(),
        image::ImageFormat::Png
    );
    let restored = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(restored.get_pixel(0, 16), source.get_pixel(16, 0));
}

#[test]
fn never_overwrites_existing_output_or_input() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([20, 30, 40, 255]))
        .save(&input)
        .unwrap();
    let before = fs::read(&input).unwrap();
    let existing = temp.path("output.png");
    fs::write(&existing, b"keep").unwrap();
    for output in [&input, &existing] {
        let result = Command::new(env!("CARGO_BIN_EXE_manga"))
            .args(["decode", "giga"])
            .arg(&input)
            .arg("--output")
            .arg(output)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("already exist"));
    }
    assert_eq!(fs::read(input).unwrap(), before);
    assert_eq!(fs::read(existing).unwrap(), b"keep");
}

#[test]
fn comici_png_respects_column_major_permutation_and_alpha() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    let output = temp.path("output.png");
    let source = image::RgbaImage::from_fn(64, 64, |x, y| image::Rgba([x as u8, y as u8, 0, 90]));
    source.save(&input).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "comici"])
        .arg(&input)
        .args(["--scramble", "[1,0,2,3,4,5,6,7,8,9,10,11,12,13,14,15]"])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let image = image::open(output).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(0, 0), source.get_pixel(0, 16));
    assert_eq!(image.get_pixel(0, 16), source.get_pixel(0, 0));
}

#[test]
fn giga_preserves_transparent_png_pixels() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    let output = temp.path("output.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([20, 30, 40, 7]))
        .save(&input)
        .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "giga"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        *image::open(output).unwrap().to_rgba8().get_pixel(0, 0),
        image::Rgba([20, 30, 40, 7])
    );
}

#[test]
fn giga_preserves_sixteen_bit_png_precision() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    let output = temp.path("output.png");
    image::ImageBuffer::<image::Rgb<u16>, Vec<u16>>::from_pixel(
        64,
        64,
        image::Rgb([12345, 23456, 34567]),
    )
    .save(&input)
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "giga"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let decoded = image::open(output).unwrap();
    assert_eq!(decoded.color(), image::ColorType::Rgb16);
    assert_eq!(
        *decoded.to_rgb16().get_pixel(0, 0),
        image::Rgb([12345, 23456, 34567])
    );
}

#[test]
fn magapoke_and_piccoma_accept_their_page_metadata() {
    let temp = TempDir::new();
    let magapoke_input = temp.path("magapoke.png");
    let magapoke_output = temp.path("magapoke-out.png");
    image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([((y / 16) * 4 + x / 16) as u8, 0, 0, 255])
    })
    .save(&magapoke_input)
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "magapoke"])
        .arg(&magapoke_input)
        .args(["--title", "1379", "--episode", "320763", "--seed", "q"])
        .arg("--output")
        .arg(&magapoke_output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        image::open(magapoke_output)
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)[0],
        11
    );

    let piccoma_input = temp.path("piccoma.png");
    let piccoma_output = temp.path("piccoma-out.png");
    image::RgbaImage::from_fn(213, 163, |x, y| {
        image::Rgba([(y / 50 * 5 + x / 50) as u8, 0, 0, 255])
    })
    .save(&piccoma_input)
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "piccoma"])
        .arg(&piccoma_input)
        .args(["--seed", "EXAMPLESEED"])
        .arg("--output")
        .arg(&piccoma_output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        image::open(piccoma_output)
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)[0],
        3
    );
}

#[test]
fn magapoke_rejects_transparency_and_unimplemented_direct_paths() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    let output = temp.path("output.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([1, 2, 3, 100]))
        .save(&input)
        .unwrap();
    let magapoke = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "magapoke"])
        .arg(&input)
        .args([
            "--direct",
            "--title",
            "1379",
            "--episode",
            "320763",
            "--seed",
            "q",
        ])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!magapoke.status.success());
    assert!(String::from_utf8_lossy(&magapoke.stderr).contains("transparent"));
    assert!(!output.exists());
    let piccoma = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "piccoma"])
        .arg(&input)
        .args(["--direct", "--seed", "EXAMPLESEED"])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!piccoma.status.success());
    assert!(String::from_utf8_lossy(&piccoma.stderr).contains("unavailable"));
    assert!(!output.exists());
}

#[test]
fn magapoke_rejects_invalid_or_ambiguous_tile_metadata() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    let output = temp.path("output.png");
    image::RgbImage::from_pixel(64, 64, image::Rgb([1, 2, 3]))
        .save(&input)
        .unwrap();
    let invalid = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "magapoke"])
        .arg(&input)
        .args(["--permutation", "[0,0]"])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(!output.exists());
    let ambiguous = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "magapoke"])
        .arg(&input)
        .args([
            "--permutation",
            "[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]",
            "--title",
            "1379",
            "--episode",
            "320763",
            "--seed",
            "q",
        ])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!ambiguous.status.success());
    assert!(!output.exists());
}

#[test]
fn yanmaga_uses_tile_keys_from_files() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    let output = temp.path("output.png");
    let source = temp.path("source");
    let destination = temp.path("destination");
    image::RgbaImage::from_fn(328, 328, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, 0, 255])
    })
    .save(&input)
    .unwrap();
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    fs::write(&source, format!("=8-8-0-HHHHHHHHHHHHHHHH{alphabet}\n")).unwrap();
    fs::write(
        &destination,
        format!(
            "=8-8+0-HHHHHHHHHHHHHHHH{}{}\n",
            &alphabet[1..],
            &alphabet[..1]
        ),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "yanmaga"])
        .arg(&input)
        .arg("--source-file")
        .arg(source)
        .arg("--destination-file")
        .arg(destination)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let actual = image::open(output).unwrap().to_rgba8();
    let original = image::open(input).unwrap().to_rgba8();
    assert_eq!(actual.get_pixel(41, 0), original.get_pixel(0, 0));
}

#[test]
fn piccoma_and_yanmaga_hold_png_with_metadata() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]))
        .save(&input)
        .unwrap();
    let mut png = fs::read(&input).unwrap();
    let end = png.len() - 12;
    // Valid tEXt chunk containing keyword 'a' and value 'b'.
    png.splice(
        end..end,
        [
            0, 0, 0, 3, b't', b'E', b'X', b't', b'a', 0, b'b', 0xdc, 0x49, 0xa2, 0x3b,
        ],
    );
    fs::write(&input, png).unwrap();
    let source = temp.path("source");
    let destination = temp.path("destination");
    fs::write(&source, "").unwrap();
    fs::write(&destination, "").unwrap();
    let output = temp.path("output.png");
    let piccoma = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "piccoma"])
        .arg(&input)
        .args(["--seed", "EXAMPLESEED"])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        !piccoma.status.success(),
        "Piccoma must not discard PNG metadata"
    );
    assert!(String::from_utf8_lossy(&piccoma.stderr).contains("metadata"));
    assert!(!output.exists());
    let yanmaga = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "yanmaga"])
        .arg(&input)
        .arg("--source-file")
        .arg(source)
        .arg("--destination-file")
        .arg(destination)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        !yanmaga.status.success(),
        "YanMaga must not discard PNG metadata"
    );
    assert!(String::from_utf8_lossy(&yanmaga.stderr).contains("metadata"));
    assert!(!output.exists());
}

#[test]
fn piccoma_and_yanmaga_preserve_png_color_type() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    image::GrayImage::from_fn(64, 64, |x, y| image::Luma([(x ^ y) as u8]))
        .save(&input)
        .unwrap();
    let source = temp.path("source");
    let destination = temp.path("destination");
    fs::write(&source, "").unwrap();
    fs::write(&destination, "").unwrap();
    let piccoma_output = temp.path("piccoma.png");
    let piccoma = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "piccoma"])
        .arg(&input)
        .args(["--seed", "EXAMPLESEED"])
        .arg("--output")
        .arg(&piccoma_output)
        .output()
        .unwrap();
    assert!(piccoma.status.success());
    assert_eq!(
        image::open(piccoma_output).unwrap().color(),
        image::ColorType::L8
    );
    let yanmaga_output = temp.path("yanmaga.png");
    let yanmaga = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "yanmaga"])
        .arg(&input)
        .arg("--source-file")
        .arg(source)
        .arg("--destination-file")
        .arg(destination)
        .arg("--output")
        .arg(&yanmaga_output)
        .output()
        .unwrap();
    assert!(yanmaga.status.success());
    assert_eq!(
        image::open(yanmaga_output).unwrap().color(),
        image::ColorType::L8
    );
}

#[test]
fn decrypts_fuz_fixture_and_rejects_bad_keys_without_output() {
    use sha2::{Digest, Sha256};
    let temp = TempDir::new();
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets/fuz-encrypted.jpeg");
    let key = temp.path("key");
    let iv = temp.path("iv");
    let output = temp.path("out.jpeg");
    fs::write(
        &key,
        "2e009856520e10917accae78097a2e13d9dd7a97d3a5ea293527ec9d0132bba3",
    )
    .unwrap();
    fs::write(&iv, "e8c7e042d6ba9fb85c128d5ceb64b82f").unwrap();
    let run = |command: &str| {
        Command::new(env!("CARGO_BIN_EXE_manga"))
            .args([command, "fuz"])
            .arg(&input)
            .arg("--key-file")
            .arg(&key)
            .arg("--iv-file")
            .arg(&iv)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap()
    };
    for command in ["decrypt", "decode"] {
        assert!(run(command).status.success());
        let result = fs::read(&output).unwrap();
        assert_eq!(
            image::guess_format(&result).unwrap(),
            image::ImageFormat::Jpeg
        );
        // Independently decrypted with OpenSSL AES-256-CBC and PKCS#7 padding.
        assert_eq!(
            Sha256::digest(&result)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            "50ed87c7cabbf5a2054307014717b9ff5ef76ff722f0c2513f597dd84a00518b"
        );
        fs::remove_file(&output).unwrap();
    }
    fs::write(&key, "invalid private key").unwrap();
    let bad = run("decrypt");
    assert!(!bad.status.success());
    assert!(!String::from_utf8_lossy(&bad.stderr).contains("invalid private key"));
    assert!(!output.exists());
}

#[test]
fn refuses_lossy_or_unsupported_restoration_without_creating_a_file() {
    let temp = TempDir::new();
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets");
    let input = assets.join("kadocomi-decrypted.webp");
    let output = temp.path("output.webp");
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "giga"])
        .arg(input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("WebP"));
    assert!(!output.exists());
}

#[test]
#[cfg(feature = "direct-sort")]
fn real_comici_unaligned_jpeg_is_held_without_output() {
    let temp = TempDir::new();
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets");
    let input = assets.join("comici-youngchampion-public-20261111.jpg");
    let data: serde_json::Value = serde_json::from_slice(
        &fs::read(assets.join("comici-youngchampion-public-20261111.permutation.json")).unwrap(),
    )
    .unwrap();
    let scramble = data["tile_sources"].to_string();
    assert_eq!(image::image_dimensions(&input).unwrap(), (844, 1200));
    let output = temp.path("output.jpg");
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "comici"])
        .arg(input)
        .arg("--direct")
        .arg("--scramble")
        .arg(scramble)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("unsupported"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists());
}

#[test]
#[cfg(feature = "direct-sort")]
fn direct_giga_restores_a_public_jpeg_without_changing_its_format() {
    use manga::{solver::ImageSolver, viewer::giga::solver::Solver};
    let temp = TempDir::new();
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/giga-tonari-yj-public-20261111.jpg");
    let output = temp.path("restored.jpg");
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "giga"])
        .arg(&input)
        .arg("--direct")
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = fs::read(output).unwrap();
    assert_eq!(
        image::guess_format(&bytes).unwrap(),
        image::ImageFormat::Jpeg
    );
    let actual = image::load_from_memory(&bytes).unwrap().into_rgb8();
    let expected = Solver::new()
        .solve_from_bytes(fs::read(input).unwrap())
        .unwrap()
        .into_rgb8();
    assert_eq!(actual, expected);
}

#[test]
#[cfg(feature = "direct-sort")]
fn direct_comici_and_magapoke_use_the_correct_jpeg_permutations() {
    use manga::{
        solver::ImageSolver,
        viewer::{comici, magapoke},
    };
    let temp = TempDir::new();
    // Synthetic metadata on a public JPEG exercises mapping without asserting its viewer identity.
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/magapoke-140481-page0-raw.jpg");
    let bytes = fs::read(&input).unwrap();
    let scrambled = "[1,0,2,3,4,5,6,7,8,9,10,11,12,13,14,15]";
    let comici_output = temp.path("comici.jpg");
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "comici"])
        .arg(&input)
        .args(["--direct", "--scramble", scrambled])
        .arg("--output")
        .arg(&comici_output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let table: comici::data::Scramble =
        serde_json::from_value(serde_json::Value::String(scrambled.into())).unwrap();
    let expected = comici::solver::Solver::new(table)
        .solve_from_bytes(bytes.clone())
        .unwrap()
        .to_rgba8();
    assert_eq!(image::open(comici_output).unwrap().to_rgba8(), expected);

    let magapoke_output = temp.path("magapoke.jpg");
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "magapoke"])
        .arg(&input)
        .args([
            "--direct",
            "--title",
            "1379",
            "--episode",
            "320763",
            "--seed",
            "q",
        ])
        .arg("--output")
        .arg(&magapoke_output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected = magapoke::restore_image(
        image::load_from_memory(&bytes).unwrap().to_rgba8(),
        1379,
        320763,
        "q",
    )
    .unwrap();
    assert_eq!(image::open(magapoke_output).unwrap().to_rgba8(), expected);
}

#[test]
#[cfg(feature = "direct-sort")]
fn direct_magapoke_restores_real_pages_with_their_recorded_tile_orders() {
    let temp = TempDir::new();
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets");
    for (episode, page) in [("140481", "page0"), ("320763", "page2")] {
        let input = assets.join(format!("magapoke-{episode}-{page}-raw.jpg"));
        let metadata: serde_json::Value = serde_json::from_slice(
            &fs::read(assets.join(format!("magapoke-{episode}.permutation.json"))).unwrap(),
        )
        .unwrap();
        let order: Vec<usize> =
            serde_json::from_value(metadata["source_tile_by_destination_row_major"].clone())
                .unwrap();
        let output = temp.path(&format!("{episode}.jpg"));
        let result = Command::new(env!("CARGO_BIN_EXE_manga"))
            .args(["decode", "magapoke"])
            .arg(&input)
            .args([
                "--direct",
                "--permutation",
                &serde_json::to_string(&order).unwrap(),
            ])
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{episode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let source = image::open(input).unwrap().to_rgb8();
        let actual = image::open(output).unwrap().to_rgb8();
        let (width, height) = source.dimensions();
        let (tile_width, tile_height) = ((width / 32) * 8, (height / 32) * 8);
        let mut expected = source.clone();
        for (destination, &tile_source) in order.iter().enumerate() {
            for y in 0..tile_height {
                for x in 0..tile_width {
                    expected.put_pixel(
                        (destination as u32 % 4) * tile_width + x,
                        (destination as u32 / 4) * tile_height + y,
                        *source.get_pixel(
                            (tile_source as u32 % 4) * tile_width + x,
                            (tile_source as u32 / 4) * tile_height + y,
                        ),
                    );
                }
            }
        }
        assert_eq!(actual, expected, "episode {episode}");
    }
}

#[test]
#[cfg(feature = "direct-sort")]
fn unsupported_subsampled_jpeg_is_not_reencoded() {
    let temp = TempDir::new();
    let input = temp.path("input.jpg");
    let output = temp.path("output.jpg");
    let mut bytes = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets/giga-tonari-yj-public-20261111.jpg"),
    )
    .unwrap();
    let mut offset = 2;
    loop {
        assert_eq!(bytes[offset], 0xff);
        let marker = bytes[offset + 1];
        if marker == 0xc0 {
            bytes[offset + 11] = 0x22;
            break;
        }
        let length = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]) as usize;
        offset += 2 + length;
    }
    fs::write(&input, bytes).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "giga"])
        .arg(&input)
        .arg("--direct")
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unsupported"));
    assert!(!output.exists());
}

#[test]
#[cfg(not(feature = "direct-sort"))]
fn disabled_direct_sort_reports_unsupported_without_output() {
    let temp = TempDir::new();
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/giga-tonari-yj-public-20261111.jpg");
    let output = temp.path("output.jpg");
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "giga"])
        .arg(input)
        .arg("--direct")
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("direct-sort"));
    assert!(!output.exists());
}

#[test]
fn rejects_wrong_format_malformed_input_and_invalid_scramble() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([20, 30, 40, 255]))
        .save(&input)
        .unwrap();
    let output = temp.path("out.jpg");
    let wrong_ext = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "giga"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!wrong_ext.status.success());
    assert!(!output.exists());
    let bad_scramble = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "comici"])
        .arg(&input)
        .args(["--scramble", "[0,0]"])
        .arg("--output")
        .arg(temp.path("out.png"))
        .output()
        .unwrap();
    assert!(!bad_scramble.status.success());
    assert!(!temp.path("out.png").exists());
    fs::write(&input, b"not an image").unwrap();
    let malformed = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "giga"])
        .arg(&input)
        .arg("--output")
        .arg(temp.path("out.png"))
        .output()
        .unwrap();
    assert!(!malformed.status.success());
    assert!(!temp.path("out.png").exists());
}

#[test]
fn webp_fallback_cli_uses_output_format_and_validates_quality() {
    let temp = TempDir::new();
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/comici-youngchampion-public-20261111.jpg");
    let meta: serde_json::Value = serde_json::from_str(include_str!(
        "assets/comici-youngchampion-public-20261111.permutation.json"
    ))
    .unwrap();
    let scramble = meta["tile_sources"].to_string();
    let output = temp.path("restored.webp");
    let run = |extra: &[&str], path: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_manga"))
            .args(["decode", "--fallback", "webp"])
            .args(extra)
            .arg("comici")
            .arg(&input)
            .args(["--scramble", &scramble, "--output"])
            .arg(path)
            .output()
            .unwrap()
    };
    assert!(run(&[], &output).status.success());
    assert_eq!(
        image::guess_format(&fs::read(&output).unwrap()).unwrap(),
        image::ImageFormat::WebP
    );
    assert!(
        !run(
            &["--lossless", "75", "--lossy", "95"],
            &temp.path("bad.webp")
        )
        .status
        .success()
    );
    assert!(
        !run(&["--lossless", "101"], &temp.path("bad.webp"))
            .status
            .success()
    );
    assert!(!run(&[], &temp.path("bad.jpg")).status.success());
}

#[test]
fn magapoke_fallback_rejects_transparent_png() {
    let temp = TempDir::new();
    let input = temp.path("input.png");
    image::RgbaImage::from_pixel(40, 40, image::Rgba([10, 20, 30, 0]))
        .save(&input)
        .unwrap();
    let output = temp.path("output.webp");
    let result = Command::new(env!("CARGO_BIN_EXE_manga"))
        .args(["decode", "--fallback", "webp", "magapoke"])
        .arg(&input)
        .args([
            "--permutation",
            "[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]",
            "--output",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
}

#[test]
fn magapoke_webp_fallback_rejects_transparency_but_accepts_opaque_pages() {
    let temp = TempDir::new();
    for (alpha, expected_success) in [(0, false), (255, true)] {
        let input = temp.path(&format!("input-{alpha}.webp"));
        image::RgbaImage::from_pixel(40, 40, image::Rgba([10, 20, 30, alpha]))
            .save(&input)
            .unwrap();
        let output = temp.path(&format!("output-{alpha}.webp"));
        let result = Command::new(env!("CARGO_BIN_EXE_manga"))
            .args(["decode", "--fallback", "webp", "magapoke"])
            .arg(&input)
            .args([
                "--permutation",
                "[1,0,2,3,4,5,6,7,8,9,10,11,12,13,14,15]",
                "--output",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            expected_success,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(output.exists(), expected_success);
    }
}
