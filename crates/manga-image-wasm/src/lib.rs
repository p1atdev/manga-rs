//! Synchronous byte-oriented ABI for JavaScript runtimes. See README.md for ownership.
use manga_image::{DirectErrorKind, ProviderImage, process_provider_image_checked};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
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
    },
    Yanmaga {
        source: String,
        destination: String,
    },
}
#[derive(Deserialize)]
struct Request {
    #[serde(flatten)]
    provider: Provider,
    width: Option<u32>,
    height: Option<u32>,
}

fn process(input: &[u8], metadata: &[u8]) -> Result<(&'static str, Vec<u8>), (u32, String)> {
    let request: Request = serde_json::from_slice(metadata)
        .map_err(|_| (4, "invalid provider metadata".to_owned()))?;
    let expected = match (request.width, request.height) {
        (None, None) => None,
        (Some(w), Some(h)) => Some((w, h)),
        _ => return Err((4, "width and height must be provided together".to_owned())),
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
        Provider::Piccoma { scrambled } => ProviderImage::Piccoma {
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
    let page = process_provider_image_checked(input, provider, expected).map_err(|e| {
        let kind = match e.kind() {
            DirectErrorKind::Unsupported => 1,
            DirectErrorKind::Malformed => 2,
            DirectErrorKind::LimitExceeded => 3,
            DirectErrorKind::InvalidArgument => 4,
            DirectErrorKind::Internal => 5,
        };
        (kind, e.to_string())
    })?;
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
}
