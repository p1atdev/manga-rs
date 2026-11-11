# cli

A CLI for downloading manga from supported official web viewers.

From the workspace root, run it through the project-local Cargo alias:

```bash
cargo manga episode <URL>
```

Downloads use directory-based WebP output under
`./<series-name>/<episode-name>/*.webp` by default. See the
[workspace README](../../README.md) for all options and examples.

Local image processing does not access the network. Specify a new output path;
the output extension must match the actual image format. Existing files are
never overwritten.

```bash
cargo manga decode giga scrambled.jpg --direct --output restored.jpg
cargo manga decode comici scrambled.png --output restored.png \
  --scramble '[1,0,2,3,4,5,6,7,8,9,10,11,12,13,14,15]'
cargo manga decode magapoke scrambled.png --output restored.png \
  --title 12345 --episode 123456 --seed SEED
cargo manga decode magapoke scrambled.jpg --output restored.jpg \
  --permutation '[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]'
cargo manga decode piccoma scrambled.png --output restored.png --seed SEED
cargo manga decode yanmaga scrambled.png --output restored.png \
  --source-file source-key.txt --destination-file destination-key.txt
cargo manga decode --fallback webp --lossless 75 comici scrambled.jpg \
  --output restored.webp --scramble '[1,0,2,3,4,5,6,7,8,9,10,11,12,13,14,15]'
cargo manga decode --fallback webp --lossy 95 piccoma scrambled.jpg \
  --output restored.webp --seed SEED
cargo manga decrypt kadokomi encrypted.bin --output image.webp --key-file key.txt
cargo manga decrypt fuz encrypted.bin --output image.jpg --key-file key.txt --iv-file iv.txt
```

The Comici array maps each destination tile to its source, using Comici's
column-major numbering. Magapoke needs the title ID, episode ID, and viewer
scramble seed, or a 16-entry row-major destination-to-source permutation;
these inputs cannot be combined. Piccoma needs the page shuffle seed;
YanMaga needs its source and destination tile keys. Keys and IVs are read from files as hexadecimal
text, so they need not appear in shell history. `decode kadokomi` and
`decode fuz` also perform their respective decryption.

Without `--fallback`, decryption writes the original image bytes without re-encoding. Giga, Comici,
and Magapoke use the shared direct backend for PNG and JPEG. It preserves
supported PNG bit depths and alpha, and rearranges supported JPEG coefficients
without recompression. PNG metadata that cannot be retained and JPEG layouts
that cannot be rearranged losslessly produce an error and no output. Magapoke
transparent images are unsupported. Piccoma and YanMaga currently support
8-bit PNG tile restoration only, and hold PNGs with metadata that cannot be
preserved. Without `--fallback`, `--direct` requires the shared direct backend
and reports unsupported for providers without a direct implementation. With
`--direct --fallback webp`, JPEG uses the direct path when available and WebP
fallback is allowed when it is not; PNG pages are encoded as WebP.

With `--fallback webp`, JPEG is first restored directly when possible. Otherwise
the decoded pixels are restored and encoded as WebP; PNG pages are also encoded
as WebP. Existing WebP bytes are retained when no restoration is required.
`--lossless` sets compression effort, while `--lossy` sets visual quality; both
accept 0 through 100 and cannot be combined. If neither is given, lossless 75 is
used. Lossless preserves decoded pixels, not the JPEG DCT coefficients. Select an
output extension matching the actual result: a mismatch fails without creating a
file. Metadata or precision that cannot be retained is held with an error,
including EXIF, XMP, PNG ancillary metadata, 16-bit PNG, and animated WebP
requiring restoration. Supported RGB JPEG ICC profiles are carried to WebP.
WebP dimensions cannot exceed 16383 pixels in either direction.

Decrypting existing JPEG or WebP bytes remains supported. The default CLI build
enables `direct-sort` for JPEG and needs a C compiler; without that feature,
direct JPEG tile restoration is unsupported, but explicit WebP fallback can
still restore supported pages. See the
[mozjpeg notices](THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/NOTICE.md) and
[libwebp license](THIRD_PARTY_NOTICES/libwebp/COPYING) and
[patent notice](THIRD_PARTY_NOTICES/libwebp/PATENTS) when
distributing the CLI binary. Run `cargo manga decode --help` and the
provider-specific `--help` for the full argument list.
