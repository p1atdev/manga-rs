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

Decryption writes the original image bytes without re-encoding. Giga, Comici,
and Magapoke use the shared direct backend for PNG and JPEG. It preserves
supported PNG bit depths and alpha, and rearranges supported JPEG coefficients
without recompression. PNG metadata that cannot be retained and JPEG layouts
that cannot be rearranged losslessly produce an error and no output. Magapoke
transparent images are unsupported. Piccoma and YanMaga currently support
8-bit PNG tile restoration only, and hold PNGs with metadata that cannot be
preserved. `--direct` requires the shared direct backend
and reports unsupported for providers without a direct implementation.

New WebP encoding is on hold, so WebP tile restoration is unsupported.
Decrypting existing JPEG or WebP bytes remains supported. The default CLI build
enables `direct-sort` for JPEG and needs a C compiler; builds made with
`--no-default-features` report JPEG tile restoration as unsupported. See the
[mozjpeg notices](THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/NOTICE.md) when
distributing the CLI binary. Run `cargo manga decode --help` and the
provider-specific `--help` for the full argument list.
