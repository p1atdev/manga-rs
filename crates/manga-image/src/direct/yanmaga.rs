// Type-F restoration adapted from the Apache-2.0 SpeedBinb implementation.
use anyhow::{Context, Result, ensure};
struct TileKey {
    padding: i64,
    horizontal: Vec<usize>,
    vertical: Vec<usize>,
    pieces: Vec<usize>,
}
fn tile_key(input: &str, sign: char) -> Result<TileKey> {
    let input = input
        .strip_prefix("=8-8")
        .context("unsupported YanMaga tile grid")?;
    let input = input
        .strip_prefix(sign)
        .context("invalid YanMaga tile direction")?;
    let (padding, data) = input.split_once('-').context("invalid YanMaga tile key")?;
    let padding: i64 = padding.parse()?;
    ensure!(
        (0..=32).contains(&padding) && data.len() == 80 && data.is_ascii(),
        "invalid YanMaga tile data"
    );
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let values: Vec<usize> = data
        .bytes()
        .map(|b| {
            alphabet
                .iter()
                .position(|v| *v == b)
                .context("invalid YanMaga tile character")
        })
        .collect::<Result<_>>()?;
    ensure!(
        values[..16].iter().all(|v| *v < 8),
        "invalid YanMaga tile edge"
    );
    let pieces = values[16..].to_vec();
    let unique: std::collections::HashSet<_> = pieces.iter().copied().collect();
    ensure!(unique.len() == 64, "invalid YanMaga tile permutation");
    Ok(TileKey {
        padding,
        horizontal: values[..8].to_vec(),
        vertical: values[8..16].to_vec(),
        pieces,
    })
}
pub fn restore_image(
    image: image::RgbaImage,
    source: &str,
    destination: &str,
) -> Result<image::RgbaImage> {
    if source.is_empty() && destination.is_empty() {
        return Ok(image);
    }
    let src = tile_key(source, '-')?;
    let dst = tile_key(destination, '+')?;
    ensure!(src.padding == dst.padding, "YanMaga tile padding mismatch");
    let (width, height) = image.dimensions();
    let (w, h, p) = (i64::from(width), i64::from(height), src.padding);
    let border = 16 * p;
    if w < 64 + border || h < 64 + border || w * h < (320 + border) * (320 + border) {
        return Ok(image);
    }
    let (cw, ch) = (w - border, h - border);
    let (pw, ph) = ((cw + 7) / 8, (ch + 7) / 8);
    let (rw, rh) = (cw - 7 * pw, ch - 7 * ph);
    ensure!(rw > 0 && rh > 0, "invalid YanMaga image dimensions");
    let mut tiles = Vec::with_capacity(64);
    for i in 0..64 {
        let (col, row) = (i % 8, i / 8);
        let target = dst.pieces[src.pieces[i]];
        let (dc, dr) = (target % 8, target / 8);
        let sx = p + col as i64 * (pw + 2 * p) + if src.vertical[row] < col { rw - pw } else { 0 };
        let sy = p
            + row as i64 * (ph + 2 * p)
            + if src.horizontal[col] < row {
                rh - ph
            } else {
                0
            };
        let tw = if src.vertical[row] == col { rw } else { pw };
        let th = if src.horizontal[col] == row { rh } else { ph };
        let dest_width = if dst.vertical[dr] == dc { rw } else { pw };
        let dest_height = if dst.horizontal[dc] == dr { rh } else { ph };
        ensure!(
            tw == dest_width && th == dest_height,
            "YanMaga tile dimensions mismatch"
        );
        let dx = dc as i64 * pw + if dst.vertical[dr] < dc { rw - pw } else { 0 };
        let dy = dr as i64 * ph + if dst.horizontal[dc] < dr { rh - ph } else { 0 };
        ensure!(
            sx >= 0
                && sy >= 0
                && dx >= 0
                && dy >= 0
                && sx + tw <= w
                && sy + th <= h
                && dx + tw <= cw
                && dy + th <= ch,
            "YanMaga tile outside image"
        );
        // Remove each tile's padding before checking coverage of the source.
        let (ux, uy) = (sx - (2 * col as i64 + 1) * p, sy - (2 * row as i64 + 1) * p);
        ensure!(
            ux >= 0 && uy >= 0 && ux + tw <= cw && uy + th <= ch,
            "YanMaga source tile outside content"
        );
        for &(other_sx, other_sy, other_dx, other_dy, other_w, other_h) in &tiles {
            let overlaps = |x: i64, y: i64, ox: i64, oy: i64| {
                x < ox + other_w && ox < x + tw && y < oy + other_h && oy < y + th
            };
            ensure!(
                !overlaps(ux, uy, other_sx, other_sy) && !overlaps(dx, dy, other_dx, other_dy),
                "overlapping YanMaga tiles"
            );
        }
        tiles.push((ux, uy, dx, dy, tw, th));
    }
    ensure!(
        tiles.iter().map(|tile| tile.4 * tile.5).sum::<i64>() == cw * ch,
        "incomplete YanMaga tile coverage"
    );
    let mut output = image::RgbaImage::new(cw as u32, ch as u32);
    for (i, (ux, uy, dx, dy, tw, th)) in tiles.into_iter().enumerate() {
        let (col, row) = (i % 8, i / 8);
        let sx = ux + (2 * col as i64 + 1) * p;
        let sy = uy + (2 * row as i64 + 1) * p;
        for y in 0..th {
            for x in 0..tw {
                output.put_pixel(
                    (dx + x) as u32,
                    (dy + y) as u32,
                    *image.get_pixel((sx + x) as u32, (sy + y) as u32),
                );
            }
        }
    }
    Ok(output)
}
