#![cfg(feature = "yanmaga")]

use image::{DynamicImage, RgbaImage};
use manga::direct::{
    ProviderImage, WebpMode, WebpOptions, process_provider_image_with_fallback_checked,
};
use std::{fs, io::Cursor, path::Path};

#[test]
fn public_yanmaga_pixels_restore_with_independent_synthetic_keys() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/yanmaga-public");
    let image = image::load_from_memory(&fs::read(assets.join("scrambled-sanitized.png")).unwrap())
        .unwrap()
        .into_rgba8();
    let mapping: serde_json::Value =
        serde_json::from_slice(&fs::read(assets.join("coordinate-mapping.json")).unwrap()).unwrap();
    let dimensions = |name: &str| -> (u32, u32) {
        let values = mapping[name].as_array().unwrap();
        (
            values[0].as_u64().unwrap().try_into().unwrap(),
            values[1].as_u64().unwrap().try_into().unwrap(),
        )
    };
    assert_eq!(image.dimensions(), dimensions("source_dimensions"));
    let (width, height) = dimensions("restored_dimensions");
    let mut reference = RgbaImage::new(width, height);
    let mut covered = vec![false; (width * height) as usize];
    let rects = mapping["rectangles_sx_sy_dx_dy_width_height"]
        .as_array()
        .unwrap();
    assert_eq!(rects.len(), 64);
    for rect in rects {
        let values: Vec<u32> = rect
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap().try_into().unwrap())
            .collect();
        let &[sx, sy, dx, dy, w, h] = values.as_slice() else {
            panic!("invalid coordinate fixture")
        };
        assert!(
            sx + w <= image.width()
                && sy + h <= image.height()
                && dx + w <= width
                && dy + h <= height
        );
        for y in 0..h {
            for x in 0..w {
                let index = ((dy + y) * width + dx + x) as usize;
                assert!(!std::mem::replace(&mut covered[index], true));
                reference.put_pixel(dx + x, dy + y, *image.get_pixel(sx + x, sy + y));
            }
        }
    }
    assert!(covered.into_iter().all(|v| v));
    // Apply a synthetic, self-inverse permutation; no original viewer keys are used.
    assert_eq!((width, height), (1127, 1600));
    let tile_width = width.div_ceil(8);
    let mut scrambled = reference.clone();
    for y in 0..height {
        for x in 0..tile_width {
            scrambled.put_pixel(x, y, *reference.get_pixel(x + tile_width, y));
            scrambled.put_pixel(x + tile_width, y, *reference.get_pixel(x, y));
        }
    }
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let source = format!(
        "=8-8-0-HHHHHHHHHHHHHHHH{}",
        std::str::from_utf8(alphabet).unwrap()
    );
    let mut permutation = *alphabet;
    for row in 0..8 {
        permutation.swap(row * 8, row * 8 + 1);
    }
    let destination = format!(
        "=8-8+0-HHHHHHHHHHHHHHHH{}",
        std::str::from_utf8(&permutation).unwrap()
    );
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(scrambled)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let output = process_provider_image_with_fallback_checked(
        png.get_ref(),
        ProviderImage::Yanmaga {
            source: &source,
            destination: &destination,
        },
        Some((width, height)),
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    )
    .unwrap();
    assert_eq!(
        image::load_from_memory(output.bytes())
            .unwrap()
            .into_rgba8(),
        reference
    );
}
