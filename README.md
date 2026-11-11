# manga-rs

An experimental Rust downloader for manga published through official web viewers.

The CLI inspects the episode page and detects its viewer implementation from the
HTML structure. It does not require the website's domain to be registered in
advance, so compatible viewers hosted on other domains can also be handled.

## Installation

```bash
cargo install --git https://github.com/p1atdev/manga-rs cli
```

## Usage

```bash
❯ manga --help
Download manga from supported official web viewers

Usage: manga <COMMAND>

Commands:
  episode  Download a single episode
  series   Download the available episodes in a supported series
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

When working from this repository, the project-local Cargo alias runs the CLI
without installing it:

```bash
cargo manga --help
cargo manga episode <URL>
```

Downloads are saved as WebP images under
`<output-dir>/<series-name>/<episode-name>/*.webp`. The current directory is
used as `<output-dir>` by default. ZIP and CBZ output follows the same hierarchy
and creates `<output-dir>/<series-name>/<episode-name>.zip` or `.cbz`.

For local files, the CLI also provides format-preserving `decode` and `decrypt`
commands. See the [CLI guide](crates/cli/README.md) for supported providers,
formats, required metadata, and examples. The `episode` and `series` download
commands retain their WebP default described above.

The common download options are:

- `-o, --output-dir <DIR>`: root directory for series and episode output (default: `.`)
- `-f, --format <FORMAT>`: `directory`, `zip`, or `cbz` (default: `directory`)
- `-i, --image-format <FORMAT>`: `png`, `jpeg`, or `webp` (default: `webp`)
- `--no-progress`: disable progress bars
- `-j, --jobs <COUNT>`: maximum concurrent image-processing jobs (default: logical CPU count)
- `--connections <COUNT>`: maximum concurrent network requests (default: `8`)

### Examples

- download an episode as a [cbz](https://en.wikipedia.org/wiki/Comic_book_archive) file

> [!TIP]
> `.cbz` is a just a `.zip` file with a different extension. You can rename it to `.zip` and extract it.

```bash
manga episode https://tonarinoyj.jp/episode/2550912964641693231 \
    --output-dir ./output \
    --format cbz
```

- download available multiple episodes as image files

```bash
manga series https://shonenjumpplus.com/episode/17106371853091617526 \
    --output-dir ./output
```

## Supported Viewers

- [x] [ChojuGiga Viewer](https://hatena.co.jp/solutions/gigaviewer) family: episode and series downloads are supported
- [x] [Kadokomi (former ComicWalker)](https://comic-walker.com): episode download is supported
- [x] [Comici Viewer](https://comici.co.jp/business/comici-plus) family: episode and series downloads are supported
- [ ] [Comic FUZ](https://comic-fuz.com): experimental guest-free viewer v2 library pipeline is available behind the `fuz` feature; CLI detection is not enabled
- [ ] [Manga Library Z](https://www.mangaz.com): deferred; library code is available behind the `mangaz` feature
- [ ] [Piccoma](https://piccoma.com): experimental guest-free episode support is available to library users behind the `piccoma` feature; CLI detection is not enabled
- [ ] [Magazine Pocket](https://pocket.shonenmagazine.com): experimental guest-free episode support is available to library users behind the `magapoke` feature; CLI detection is not enabled
- [ ] [YanMaga](https://yanmaga.jp): experimental anonymous full-episode support is available to library users behind the `yanmaga` feature; CLI detection is not enabled

## Architecture

- `crates/cli`: detects the viewer and dispatches to the matching download pipeline.
- `crates/manga/src/viewer`: viewer-specific page parsing, API access, and image solving.
- `crates/manga/src/pipeline.rs`: shared concurrent fetch, solve, and write orchestration.
- `crates/manga/src/io`: raw-directory and ZIP/CBZ writers.

The `manga` crate enables `comici`, `giga`, and `kadokomi` by default. Live-site smoke tests
are ignored during normal test runs; fixture-based parsing and image-processing
tests run offline.

### Format-preserving page processing

`manga::direct::process_direct` accepts already acquired bytes and restoration
metadata and returns a `ProcessedPage` with the processed bytes, actual format,
MIME type, and extension. It does not fetch images or impose an acquisition-size
limit. The caller chooses the viewer and handles acquisition, storage, and any
page errors. For example:

```rust,ignore
use manga::direct::{process_direct, DirectTransform};

let page = process_direct(&downloaded_bytes, DirectTransform::Giga)?;
let mime_type = page.mime_type();
let bytes = page.into_bytes();
// Store `bytes` with `mime_type`.
```

`Preserve` retains a validated JPEG, PNG, or WebP byte-for-byte when the caller
has established that no restoration is needed. `Kadocomi` decrypts and retains
the decrypted format and bytes. `Giga` applies the GigaViewer layout, and
`Tiles4x4` accepts a destination-to-source, row-major permutation supplied by
the caller. The CLI supports Magapoke using a supplied permutation or its
title ID, episode ID, and seed.

JPEG tile restoration requires the optional `direct-sort` library feature. It
moves quantized DCT blocks without requantizing, but supports only baseline
8-bit grayscale and YCbCr 4:4:4 JPEGs with supported metadata. Subsampled or
progressive JPEGs, non-aligned JPEG tiles, EXIF, and other unsupported metadata
return a reasoned error. The caller should hold those pages; this API does not
recompress JPEGs or switch their format. Huffman coding may change the output
bytes even when the coefficients are preserved.

PNG tile restoration losslessly re-encodes non-indexed 8-bit or 16-bit grayscale,
grayscale-alpha, RGB, and RGBA images. It preserves pixel values, bit depth, and
alpha, but cannot yet preserve ancillary metadata. PNGs with ancillary chunks,
palettes, or unsupported bit depths are held with a reason instead of silently
dropping metadata. PNG chunk CRCs are checked. The strict API still holds
scrambled WebP; an unscrambled WebP can pass through `Preserve` unchanged.
An opt-in WebP fallback is available through
`process_provider_image_with_fallback_checked` and the CLI's `--fallback webp`.
It retains directly restorable JPEGs and unchanged WebP, and encodes other
supported pages as WebP. Lossless quality controls compression effort and
preserves decoded pixels; lossy quality controls visual fidelity. Unsupported
metadata, animation and 16-bit PNG are reported rather than silently discarded.
Validated RGB JPEG ICC profiles are preserved in WebP. Limits inherent in the image libraries,
integer representation, and available memory still apply.

Enable JPEG coefficient processing in a consuming crate with:

```toml
manga = { path = "path/to/manga/crates/manga", features = ["direct-sort"] }
```

The feature statically builds the bundled `mozjpeg-sys` 2.2.3 C sources and
requires a native C toolchain. It is not enabled by the `manga` library's
default features; when disabled, JPEG tile restoration returns an explanatory
unsupported error. The configured build disables the dependency's optional SIMD
features. macOS arm64 was tested; Linux and Windows builds have not been
verified for this change. Distributions containing the feature must include
the [third-party notices](THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/NOTICE.md).
The same notice files are included in the `manga` crate package under
`THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/`. In the repository the root and crate
copies are kept identical; see the notice for the comparison command. Binary
release packaging must include the notices separately.
The crate package also includes a copy of the repository's Apache-2.0
`LICENSE`; check the two copies with `cmp LICENSE crates/manga/LICENSE` from
the repository root when updating it.

The JPEG fixture provenance is recorded beside the files in
`crates/manga/tests/assets/`: the Tonari no Young Jump fixture and the
`*-raw.jpg` samples retain the acquired JPEG bytes, while the first two Jump+
and first two Kurage Bunch samples were normalized with `jpegtran -copy none`.
The normalized files' coefficients were not compared against the unavailable
original bytes. Magapoke permutation files contain destination-to-source tile
indices; the fixture tests do not imply that provider integration is complete.

### Direct JPEG benchmark

The direct and previous JPEG paths can be compared on a local fixture without
network or file writes inside the timed operations:

```sh
cargo run --release -p manga --features direct-sort --example direct_sort_bench -- \
    crates/manga/tests/assets/giga-tonari-yj-public-20261111.jpg
cargo run --release -p manga --features direct-sort --example direct_sort_bench -- \
    crates/manga/tests/assets/jumpplus-page2-raw.jpg
```

The example warms both paths and measures 100 ABBA blocks, one operation per
interval. It reports mean milliseconds and a Student-t 95% interval for the
paired percentage improvement. The comparison is format-matched JPEG output,
but the previous decode/solve/JPEG-encode path adds lossy recompression; its
quality is therefore different from the coefficient path. This is a local
microbenchmark, not a network or end-to-end download measurement.

On an Apple M1 Ultra with Rust 1.99.0, an earlier build of this implementation
measured the 800×1137 color fixture at 15.2657 ms versus 12.2533 ms: 19.734%
mean improvement, 95% interval 19.552–19.915%. The 822×1200 raw grayscale
fixture measured 14.4904 ms versus 8.6286 ms: 40.449%, interval
40.275–40.622%. These measurements preceded the later limit and PNG validation
changes and used a temporary harness dependency lock; they are not a fresh
measurement of the current full build. Shared-host noise and different
dependencies can change results. x86 timing was not measured.

### Image solver benchmarks

Run the ignored release benchmark on the included public GigaViewer JPEG with:

```sh
cargo test --release -p manga benchmark_giga_public_jpeg_paired_abba -- --ignored --nocapture --test-threads=1
```

The benchmark checks pixel equality against the original solver and reports
100 paired ABBA blocks for restoration alone and JPEG decoding plus
restoration. Each measurement interval lasts at least 50 ms. It includes the
same input cloning in both variants and excludes network and file writes.
The confidence intervals are descriptive and can be affected by shared-host
noise. The fixture's provenance is in the adjacent metadata file.

### Experimental Piccoma library support

Enable the `manga` crate's `piccoma` feature and use
`manga::viewer::piccoma::Pipeline` with the shared `EpisodePipeline` trait.
The pipeline checks that the episode is explicitly marked free in the guest
listing before opening its viewer. Waiting, paid, and ambiguous episodes are
not supported. Its HTML parser depends on the current viewer format; live
smoke tests are ignored by default. Image restoration has been exercised
against synthetic fixtures and live page dimensions, but pixel equality to
the official viewer is not yet independently verified.

### Experimental Magazine Pocket library support

Enable the `manga` crate's `magapoke` feature and use
`manga::viewer::magapoke::Pipeline` with the shared `EpisodePipeline` trait.
The guest detail API must explicitly confirm that an episode is free before
the viewer is requested; ticketed, subscription and ambiguous episodes are
not supported. Images with transparency are rejected until their official
canvas compositing is reproduced. Episode output names currently use the
episode ID. Live tests are ignored by default.

### Experimental YanMaga library support

Enable the `manga` crate's `yanmaga` feature and use
`manga::viewer::yanmaga::Pipeline` with the shared `EpisodePipeline` trait.
The pipeline uses an anonymous cookie session and rejects viewer responses
that provide only a trial or deny access. Episode output names currently use
the episode ID. The viewer protocol may change; live tests are ignored by
default. The feature enables reqwest's `cookies` support.

### Experimental COMIC FUZ viewer v2

With the `fuz` feature, use `manga::viewer::fuz::v2::Pipeline::for_work`
and the shared `EpisodePipeline` trait. The work ID is required so the
pipeline can confirm the chapter is explicitly guest-free before requesting
its viewer. Each episode uses an isolated anonymous image-token session.
The legacy FUZ API remains available. The feature enables reqwest cookies.
Live tests are ignored by default and the viewer protocol may change.
