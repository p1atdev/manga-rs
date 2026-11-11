# mozjpeg-sys 2.2.3 notices

The optional `direct-sort` feature statically links the C library bundled by
`mozjpeg-sys` 2.2.3. The crate declares `IJG AND Zlib AND BSD-3-Clause`.

This software is based in part on the work of the Independent JPEG Group.

- `README.ijg` is the unmodified upstream IJG README, including its license.
- `UPSTREAM-LICENSE.md` is the unmodified upstream license document, including
  the Modified 3-clause BSD license and distribution guidance.
- `ZLIB-NOTICE.txt` preserves the copyright and zlib license notice from the
  upstream `simd/nasm/jsimdext.inc` file. SIMD is disabled in this configuration;
  this notice is included with the other upstream licensing material.
- `CRATE-LICENSE` is the `LICENSE` file shipped in the published crate.

Source versions:

- [mozjpeg-sys 2.2.3](https://crates.io/crates/mozjpeg-sys/2.2.3), repository
  revision `93e9c78d0e9afb018a224e1105f06e48aec77766`.
- Its `vendor` submodule is [mozilla/mozjpeg](https://github.com/mozilla/mozjpeg)
  revision `c2bc3511d547870c4f67bbafda5fba794ba71f7f`. `README.ijg` and
  `UPSTREAM-LICENSE.md` are copied from that exact revision. The zlib notice is
  copied from that revision's `simd/nasm/jsimdext.inc`; the published crate's
  copy of that file was checked against the upstream file.

When distributing a binary with `direct-sort`, include this notice and the
accompanying license documents with the distribution. The IJG README also
contains requirements for distributions that include source code.

In this repository, this directory is mirrored byte-for-byte under
`crates/manga/THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/` so the crate package
also includes the notices. Update both copies together. From the repository
root, check that they agree with:

```sh
diff -rq THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3 crates/manga/THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3
```
