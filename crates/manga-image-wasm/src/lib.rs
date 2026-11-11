//! Synchronous byte-oriented ABI for JavaScript runtimes. See README.md for ownership.
use manga_image::{
    DirectError, DirectErrorKind, ProviderImage, WebpMode, WebpOptions,
    process_provider_image_checked, process_provider_image_with_fallback_checked,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Provider {
    Giga,
    Comici {
        scramble: Vec<usize>,
    },
    Magapoke {
        title: u32,
        episode: u32,
        seed: String,
    },
    FuzRaw,
    FuzAesCbc {
        #[serde(rename = "keyHex")]
        key_hex: String,
        #[serde(rename = "ivHex")]
        iv_hex: String,
    },
    KadokomiRaw,
    KadokomiXor {
        #[serde(rename = "keyHex")]
        key_hex: String,
    },
    Piccoma {
        scrambled: bool,
        seed: Option<String>,
    },
    Yanmaga {
        source: String,
        destination: String,
    },
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FallbackFormat {
    Webp,
}
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FallbackMode {
    Lossless,
    Lossy,
}
/// Optional WebP re-encode fallback, passed to core as `WebpOptions`.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fallback {
    #[allow(dead_code)]
    format: FallbackFormat,
    mode: FallbackMode,
    quality: u8,
}

/// Request-level keys; the rest of the object belongs to the provider variant.
/// `#[serde(flatten)]` cannot be combined with `deny_unknown_fields`, so the
/// split is done by hand to keep unknown fields fail-closed.
struct Request {
    provider: Provider,
    width: Option<u32>,
    height: Option<u32>,
    fallback: Option<Fallback>,
}

const INVALID: &str = "invalid provider metadata";

fn bad<T>(message: &str) -> Result<T, (u32, String)> {
    Err((4, message.to_owned()))
}

fn parse_request(metadata: &[u8]) -> Result<Request, (u32, String)> {
    use serde_json::Value;
    let Ok(Value::Object(mut map)) = serde_json::from_slice::<Value>(metadata) else {
        return bad(INVALID);
    };
    let mut dimension = |key: &str| -> Result<Option<u32>, (u32, String)> {
        match map.remove(key) {
            None => Ok(None),
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .or_else(|_| bad(INVALID)),
        }
    };
    let width = dimension("width")?;
    let height = dimension("height")?;
    let fallback = match map.remove("fallback") {
        None => None,
        Some(v) => {
            let f: Fallback = serde_json::from_value(v).or_else(|_| bad("invalid fallback"))?;
            if f.quality > 100 {
                return bad("invalid fallback");
            }
            Some(f)
        }
    };
    // serde ignores extra fields on unit variants of internally tagged enums even
    // with `deny_unknown_fields`, so the allowed key set is enforced by hand.
    let allowed: &[&str] = match map.get("kind").and_then(Value::as_str) {
        Some("giga" | "fuz_raw" | "kadokomi_raw") => &[],
        Some("comici") => &["scramble"],
        Some("magapoke") => &["title", "episode", "seed"],
        Some("fuz_aes_cbc") => &["keyHex", "ivHex"],
        Some("kadokomi_xor") => &["keyHex"],
        Some("piccoma") => &["scrambled", "seed"],
        Some("yanmaga") => &["source", "destination"],
        _ => return bad(INVALID),
    };
    if map
        .keys()
        .any(|k| k != "kind" && !allowed.contains(&k.as_str()))
    {
        return bad(INVALID);
    }
    let provider: Provider =
        serde_json::from_value(Value::Object(map)).or_else(|_| bad(INVALID))?;
    if let Provider::Piccoma { scrambled, seed } = &provider {
        match seed {
            Some(seed)
                if !*scrambled || seed.is_empty() || seed.len() > 256 || !seed.is_ascii() =>
            {
                return bad("invalid Piccoma seed");
            }
            _ => {}
        }
    }
    Ok(Request {
        provider,
        width,
        height,
        fallback,
    })
}

fn error_tuple(e: DirectError) -> (u32, String) {
    let kind = match e.kind() {
        DirectErrorKind::Unsupported => 1,
        DirectErrorKind::Malformed => 2,
        DirectErrorKind::LimitExceeded => 3,
        DirectErrorKind::InvalidArgument => 4,
        DirectErrorKind::Internal => 5,
    };
    (kind, e.to_string())
}

fn process(input: &[u8], metadata: &[u8]) -> Result<(&'static str, Vec<u8>), (u32, String)> {
    let request = parse_request(metadata)?;
    let expected = match (request.width, request.height) {
        (None, None) => None,
        (Some(w), Some(h)) => Some((w, h)),
        _ => return bad("width and height must be provided together"),
    };
    let scramble = if let Provider::Comici { ref scramble } = request.provider {
        let entries: [usize; 16] = scramble
            .as_slice()
            .try_into()
            .map_err(|_| (4, "invalid Comici scramble".to_owned()))?;
        Some(
            manga_image::viewer::comici::data::Scramble::try_from(entries)
                .map_err(|_| (4, "invalid Comici scramble".to_owned()))?,
        )
    } else {
        None
    };
    let provider = match &request.provider {
        Provider::Giga => ProviderImage::Giga,
        Provider::Comici { .. } => ProviderImage::Comici {
            scramble: scramble.as_ref().unwrap(),
        },
        Provider::Magapoke {
            title,
            episode,
            seed,
        } => ProviderImage::Magapoke {
            title: *title,
            episode: *episode,
            seed,
        },
        Provider::FuzRaw => ProviderImage::FuzRaw,
        Provider::FuzAesCbc { key_hex, iv_hex } => ProviderImage::FuzAesCbc { key_hex, iv_hex },
        Provider::KadokomiRaw => ProviderImage::KadokomiRaw,
        Provider::KadokomiXor { key_hex } => ProviderImage::KadokomiXor { key_hex },
        // Seed is only accepted for scrambled pages (checked in `parse_request`).
        Provider::Piccoma {
            scrambled: true,
            seed: Some(seed),
        } => ProviderImage::PiccomaSeed { seed },
        Provider::Piccoma { scrambled, .. } => ProviderImage::Piccoma {
            scrambled: *scrambled,
        },
        Provider::Yanmaga {
            source,
            destination,
        } => ProviderImage::Yanmaga {
            source,
            destination,
        },
    };
    let page = match &request.fallback {
        Some(f) => {
            let options = WebpOptions {
                mode: match f.mode {
                    FallbackMode::Lossless => WebpMode::Lossless,
                    FallbackMode::Lossy => WebpMode::Lossy,
                },
                quality: f.quality,
            };
            process_provider_image_with_fallback_checked(input, provider, expected, options)
        }
        None => process_provider_image_checked(input, provider, expected),
    }
    .map_err(error_tuple)?;
    Ok((page.mime_type(), page.into_bytes()))
}

// C codecs and Rust must use the same allocator: two independent allocators on
// wasm32-unknown-unknown otherwise both start at __heap_base and corrupt each other.
#[cfg(target_arch = "wasm32")]
mod shared_allocator {
    use std::alloc::{GlobalAlloc, Layout};
    use std::ffi::c_void;
    unsafe extern "C" {
        fn posix_memalign(out: *mut *mut c_void, alignment: usize, size: usize) -> i32;
        fn free(ptr: *mut c_void);
    }
    pub struct WasiAllocator;
    unsafe impl GlobalAlloc for WasiAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let mut ptr = std::ptr::null_mut();
            let alignment = layout.align().max(std::mem::size_of::<usize>());
            if unsafe { posix_memalign(&mut ptr, alignment, layout.size()) } != 0 {
                return std::ptr::null_mut();
            }
            ptr.cast()
        }
        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { free(ptr.cast()) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: WasiAllocator = WasiAllocator;
}

#[cfg(target_arch = "wasm32")]
mod abi {
    use super::*;
    use std::cell::RefCell;
    thread_local! { static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) }; static FORMAT: RefCell<u32> = const { RefCell::new(0) }; }
    #[unsafe(no_mangle)]
    pub extern "C" fn alloc(len: usize) -> *mut u8 {
        let mut v = Vec::new();
        if v.try_reserve_exact(len).is_err() {
            return std::ptr::null_mut();
        }
        v.resize(len, 0);
        Box::into_raw(v.into_boxed_slice()) as *mut u8
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn dealloc(p: *mut u8, len: usize) {
        if !p.is_null() {
            unsafe { drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, len))) }
        }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn restore(
        input: *const u8,
        input_len: usize,
        metadata: *const u8,
        metadata_len: usize,
    ) -> u32 {
        let input = unsafe { std::slice::from_raw_parts(input, input_len) };
        let metadata = unsafe { std::slice::from_raw_parts(metadata, metadata_len) };
        let (status, format, bytes) = match process(input, metadata) {
            Ok((mime, bytes)) => (
                0,
                match mime {
                    "image/jpeg" => 1,
                    "image/png" => 2,
                    "image/webp" => 3,
                    _ => 0,
                },
                bytes,
            ),
            Err((code, message)) => (code, 0, message.into_bytes()),
        };
        OUTPUT.with(|out| *out.borrow_mut() = bytes);
        FORMAT.with(|value| *value.borrow_mut() = format);
        status
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn output_ptr() -> *const u8 {
        OUTPUT.with(|out| out.borrow().as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn output_len() -> usize {
        OUTPUT.with(|out| out.borrow().len())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn output_format() -> u32 {
        FORMAT.with(|value| *value.borrow())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn clear_output() {
        OUTPUT.with(|out| *out.borrow_mut() = Vec::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_and_image_errors_are_classified() {
        let input = include_bytes!("../../manga/tests/assets/giga-tonari-yj-public-20261111.jpg");
        let (format, output) = process(input, br#"{"kind":"giga"}"#).unwrap();
        assert_eq!(format, "image/jpeg");
        assert!(!output.is_empty());
        assert_eq!(
            process(&input[..input.len() / 2], br#"{"kind":"giga"}"#)
                .unwrap_err()
                .0,
            2
        );
        assert_eq!(
            process(input, br#"{"kind":"piccoma","scrambled":true}"#)
                .unwrap_err()
                .0,
            1
        );
        assert_eq!(
            process(input, br#"{"kind":"giga","width":0,"height":1}"#)
                .unwrap_err()
                .0,
            4
        );
    }

    fn code(metadata: &str) -> u32 {
        let input = include_bytes!("../../manga/tests/assets/giga-tonari-yj-public-20261111.jpg");
        // Never unwrap_err: on unexpected success it would Debug-print image bytes.
        match process(input, metadata.as_bytes()) {
            Err((code, _)) => code,
            Ok((format, output)) => {
                panic!(
                    "unexpected success for {metadata}: {format}, {} bytes",
                    output.len()
                )
            }
        }
    }

    #[test]
    fn invalid_metadata_fails_closed() {
        for m in [
            "",
            "[]",
            "null",
            r#"{}"#,
            r#"{"kind":"nope"}"#,
            r#"{"kind":"giga","extra":1}"#,
            r#"{"kind":"giga","width":"1","height":1}"#,
            r#"{"kind":"giga","width":1}"#,
            r#"{"kind":"giga","fallback":null}"#,
            r#"{"kind":"giga","fallback":{}}"#,
            r#"{"kind":"giga","fallback":{"format":"png","mode":"lossless","quality":1}}"#,
            r#"{"kind":"giga","fallback":{"format":"webp","mode":"fast","quality":1}}"#,
            r#"{"kind":"giga","fallback":{"format":"webp","mode":"lossy","quality":101}}"#,
            r#"{"kind":"giga","fallback":{"format":"webp","mode":"lossy","quality":-1}}"#,
            r#"{"kind":"giga","fallback":{"format":"webp","mode":"lossy","quality":1.5}}"#,
            r#"{"kind":"giga","fallback":{"format":"webp","mode":"lossy","quality":"5"}}"#,
            r#"{"kind":"giga","fallback":{"format":"webp","mode":"lossy","quality":5,"x":1}}"#,
        ] {
            assert_eq!(code(m), 4, "{m}");
        }
    }

    #[test]
    fn fallback_options_parse() {
        for (mode, q) in [("lossless", 0), ("lossy", 100), ("lossy", 75)] {
            let m = format!(
                r#"{{"kind":"giga","width":3,"height":4,"fallback":{{"format":"webp","mode":"{mode}","quality":{q}}}}}"#
            );
            let r = parse_request(m.as_bytes()).unwrap();
            assert_eq!((r.width, r.height), (Some(3), Some(4)));
            let f = r.fallback.unwrap();
            assert_eq!(f.quality, q);
            assert_eq!(f.mode == FallbackMode::Lossless, mode == "lossless");
        }
        assert!(
            parse_request(br#"{"kind":"giga"}"#)
                .unwrap()
                .fallback
                .is_none()
        );
    }

    fn fallback_meta(kind: &str, mode: &str, quality: u8) -> String {
        format!(
            r#"{{"kind":"{kind}","fallback":{{"format":"webp","mode":"{mode}","quality":{quality}}}}}"#
        )
    }

    #[test]
    fn direct_jpeg_is_stable_with_either_fallback_setting() {
        let input = include_bytes!("../../manga/tests/assets/giga-tonari-yj-public-20261111.jpg");
        let (format, plain) = process(input, br#"{"kind":"giga"}"#).unwrap();
        assert_eq!(format, "image/jpeg");
        for (mode, q) in [("lossless", 100), ("lossy", 75)] {
            let m = fallback_meta("giga", mode, q);
            let (format, out) = process(input, m.as_bytes()).unwrap();
            assert_eq!(format, "image/jpeg", "{mode}");
            assert!(
                out == plain,
                "{mode}: output differs from no-fallback output"
            );
        }
    }

    #[test]
    fn existing_webp_is_byte_identical_with_fallback() {
        let input = include_bytes!("../../manga/tests/assets/kadocomi-decrypted.webp");
        for (mode, q) in [("lossless", 100), ("lossy", 75)] {
            let m = fallback_meta("kadokomi_raw", mode, q);
            let (format, out) = process(input, m.as_bytes()).unwrap();
            assert_eq!(format, "image/webp", "{mode}");
            assert!(out == input, "{mode}: webp bytes changed");
        }
    }

    #[test]
    fn fallback_converts_png_to_webp() {
        // Tiny deterministic 16x16 RGBA PNG built in memory (well under core pixel caps).
        let mut png = Vec::new();
        image::RgbaImage::from_fn(16, 16, |x, y| {
            image::Rgba([
                (x * 16) as u8,
                (y * 16) as u8,
                ((x + y) * 8) as u8,
                255 - (x as u8),
            ])
        })
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        let png = png.as_slice();
        let (format, plain) = process(png, br#"{"kind":"giga"}"#).unwrap();
        assert_eq!(format, "image/png");
        assert!(plain.starts_with(b"\x89PNG"));
        for (mode, q) in [("lossless", 100), ("lossy", 75)] {
            let m = fallback_meta("giga", mode, q);
            let (format, out) = process(png, m.as_bytes()).unwrap();
            assert_eq!(format, "image/webp", "{mode}");
            assert!(
                out.len() > 12 && &out[..4] == b"RIFF" && &out[8..12] == b"WEBP",
                "{mode}"
            );
            let riff_len = u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize;
            assert_eq!(riff_len + 8, out.len(), "{mode}: RIFF size mismatch");
        }
    }

    #[test]
    fn fallback_errors_are_classified() {
        let input = include_bytes!("../../manga/tests/assets/giga-tonari-yj-public-20261111.jpg");
        let m = fallback_meta("giga", "lossy", 75);
        match process(&input[..input.len() / 2], m.as_bytes()) {
            Err((code, _)) => assert_eq!(code, 2),
            Ok(_) => panic!("truncated input must not succeed"),
        }
        assert_eq!(
            code(
                r#"{"kind":"giga","width":0,"height":1,"fallback":{"format":"webp","mode":"lossless","quality":1}}"#
            ),
            4
        );
        // Scrambled Piccoma without a seed stays unsupported, with or without fallback.
        assert_eq!(
            code(
                r#"{"kind":"piccoma","scrambled":true,"fallback":{"format":"webp","mode":"lossy","quality":5}}"#
            ),
            1
        );
    }

    #[test]
    fn piccoma_seed_handling() {
        let seed = |m| parse_request(m).map(|r| r.provider);
        assert!(matches!(
            seed(br#"{"kind":"piccoma","scrambled":true,"seed":"abc"}"#),
            Ok(Provider::Piccoma {
                scrambled: true,
                seed: Some(_)
            })
        ));
        assert!(matches!(
            seed(br#"{"kind":"piccoma","scrambled":true}"#),
            Ok(Provider::Piccoma { seed: None, .. })
        ));
        assert_eq!(code(r#"{"kind":"piccoma","scrambled":true,"seed":""}"#), 4);
        assert_eq!(
            code(r#"{"kind":"piccoma","scrambled":false,"seed":"abc"}"#),
            4
        );
        assert_eq!(code(r#"{"kind":"piccoma","scrambled":true,"seed":1}"#), 4);
        assert_eq!(code(r#"{"kind":"piccoma","scrambled":true,"seed":"é"}"#), 4);
        let long = |n: usize| {
            format!(
                r#"{{"kind":"piccoma","scrambled":true,"seed":"{}"}}"#,
                "a".repeat(n)
            )
        };
        assert_eq!(code(&long(257)), 4);
        assert!(parse_request(long(256).as_bytes()).is_ok());
        // Existing behavior preserved.
        assert_eq!(code(r#"{"kind":"piccoma","scrambled":true}"#), 1);
    }

    #[test]
    fn direct_bytes_are_preserved_and_format_errors_not_success() {
        let input = include_bytes!("../../manga/tests/assets/giga-tonari-yj-public-20261111.jpg");
        let (_, a) = process(input, br#"{"kind":"giga"}"#).unwrap();
        let (_, b) = process(input, br#"{"kind":"giga"}"#).unwrap();
        assert_eq!(a, b);
        for bad_input in [&b""[..], b"not an image", &input[..input.len() / 2]] {
            assert!(process(bad_input, br#"{"kind":"giga"}"#).is_err());
        }
    }
}
