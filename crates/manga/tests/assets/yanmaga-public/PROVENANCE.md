# YanMaga public-page fixture

- Source work: https://yanmaga.jp/comics/%E5%85%AC%E5%BC%8F%E4%B8%8D%E5%80%AB
- Page: first image of 第１話, displayed as 無料 and not modal-blocked in the public work listing; the anonymous viewer returned full-access mode.
- Retrieved: 2026-11-12 JST; one image downloaded for this export.
- `yanmaga-public-scrambled-sanitized.png` is a losslessly re-encoded RGBA raster of the downloaded scrambled image, not its original file bytes. Its only PNG chunks are IHDR, IDAT and IEND. Original image metadata was not retained.
- In this directory the image is named `scrambled-sanitized.png` and the mapping `coordinate-mapping.json`. The original delivery format was not verified; this PNG is a re-encoded raster.
- `yanmaga-public-coordinate-mapping.json` contains image dimensions and 64 numeric rectangles `[source_x, source_y, destination_x, destination_y, width, height]`. The rectangles were extracted by applying the pre-existing YanMaga solver to an in-memory coordinate-coded raster. Applying them to the saved image reproduced every RGBA pixel from that solver.
- Viewer cookies, signed image URLs, source/destination keys and other viewer metadata were never written to these files. The JSON contains only nonnegative integers, dimensions and coordinate field names.
- The test applies the recorded coordinate mapping to reconstruct the reference image, then scrambles it with a separate synthetic, self-inverse permutation and exercises the WebP fallback. It does not use or recover the original viewer keys and does not test the original delivery encoding.
