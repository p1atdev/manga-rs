# Image processing in Workers

`manga-image` is a network-independent Rust library. Its strict JPEG validation always builds the bundled mozjpeg C dependency, including when the `direct-sort` feature is disabled, so a compatible C compiler is required for every configuration. The `manga` crate re-exports its existing `direct` API from that library. This crate builds the synchronous Wasm ABI and the small ES module in `js/index.mjs`. The caller is responsible for fetching image data and provider metadata; neither library performs a network request or imposes URL restrictions.

## Build

Run from the repository root. Install Rust with `wasm32-unknown-unknown`, a clang that supports `-mllvm -wasm-enable-sjlj`, and a WASI libc sysroot containing `lib/wasm32-wasip1/libc.a` and `libsetjmp.a`. The tested Mac toolchain was Rust 1.99.0, clang 23.1.2, and Homebrew's wasi-libc 34 sysroot (built from `wasi-sdk-34`, source archive SHA256 `21bc47f96cf0eb535e532697dce4cdfdbc91f59223bdc00d8495d191888c5fe8`).

```sh
export WASM_CLANG=/path/to/clang
export WASI_SYSROOT=/path/to/wasi-sysroot
crates/manga-image-wasm/scripts/build.sh
```

The artifact is `target/wasm32-unknown-unknown/release/manga_image_wasm.wasm`. Cargo uses the repository's `Cargo.lock` and `--locked`. For crates.io publication, release `manga-image` 0.1.0 first and wait until it is available in the registry before packaging or publishing `manga` or `manga-image-wasm`; their path dependencies specify that same version. The mozjpeg and libwebp distributions carry their own licenses and must be included when redistributing binaries. A consuming CI should check out a fixed, reviewed revision of this repository, pin the Rust, clang, and sysroot distributions and verify their checksums, run this script, and package that artifact together with `js/index.mjs`. Do not depend on a build machine's user directory or an uncommitted working tree. A reproducible pinned Linux CI image and its build have **not yet been verified**; matching version labels alone does not guarantee identical bytes or behavior across platforms.

The libc `libsetjmp` and SjLj compiler flag are necessary for the JPEG C shim's local error handling. C codecs and Rust share the WASI libc allocator; two separate allocators would overlap in Wasm linear memory. WASI imports are **not** implemented: each such call throws explicitly and a trapped instance is retired. A successful fixture run does not prove every input avoids those calls. A successful result never converts the image format. Scrambled WebP, unsupported JPEG encodings/metadata, and other unsupported page cases return `unsupported` for the caller to hold.

## Interface

Supply the compiled Wasm module as a Worker module binding:

```js
import wasmModule from './manga_image_wasm.wasm';
import { createImageProcessor } from './index.mjs';
const restore = createImageProcessor(wasmModule);
const result = restore(inputBytes, { kind: 'giga', width: 800, height: 1137 });
// result: { kind: 'ok', bytes: Uint8Array, contentType: 'image/jpeg' | 'image/png' | 'image/webp' }
//      or { kind: 'unsupported', reason: string }
```

`restore` is synchronous and requires a `Uint8Array`. It copies the output before releasing the internal buffer. The same instance may be reused; avoid exposing raw ABI exports. Other failures throw an `Error` whose `code` is `malformed`, `limit_exceeded`, `invalid_argument`, or `internal`; a Wasm/WASI trap is also thrown and invalidates the instance. `restore.memoryBytes()` reports currently allocated Wasm linear-memory bytes for diagnostics, not request peak memory. Do not expose metadata containing decryption keys in logs.

Accepted metadata objects (exact property spelling):

- `{kind:'giga'}`
- `{kind:'comici', scramble: [/* permutation of 0..15 */]}`
- `{kind:'magapoke', title: 1379, episode: 320763, seed: 'q'}`
- `{kind:'fuz_raw'}` or `{kind:'fuz_aes_cbc', keyHex: '...', ivHex: '...'}`
- `{kind:'kadokomi_raw'}` or `{kind:'kadokomi_xor', keyHex: '...'}`
- `{kind:'piccoma', scrambled: false}` (scrambled pages are held)
- `{kind:'yanmaga', source: '', destination: ''}` (nonempty maps are held)

Add `width` and `height` together only when both dimensions are known. Zero or missing one is an invalid argument; a mismatch is malformed. `keyHex` and `ivHex` are secret provider metadata.

## Local verification

```sh
cargo test -p manga-image --all-features
cargo test -p manga-image-wasm --lib
npm ci --prefix crates/manga-image-wasm
node crates/manga-image-wasm/scripts/workerd-test.mjs
```

The last command uses local Miniflare/workerd, the real Giga JPEG and Kadokomi WebP fixtures, malformed and unsupported cases, and 120 overlapping requests sharing one instance. Set `WORKERD_OUTPUT` and `WORKERD_TWICE_OUTPUT` to save the two JPEG outputs if needed. It does not contact the remote Workers service.

## Resource limits

The decoder deliberately does not add a new image byte-size or pixel limit. Memory use depends on the input and may include compressed input, decoded pixels, C codec buffers, and output copies; Wasm memory cannot shrink after growth. Workers have their own memory and CPU limits, and an allocator abort or runtime trap may not be recoverable as a typed error. The test's observed memory is fixture-specific, not a general upper bound. Callers must treat thrown errors and unsupported responses as failures and avoid publishing a page as successfully restored.
