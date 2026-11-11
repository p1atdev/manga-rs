//! Anonymous YanMaga SpeedBinb viewer.
//! Tile-table decoding and type-F layout are informed by the Apache-2.0
//! keiyoushi/extensions-source SpeedBinb implementation:
//! https://github.com/keiyoushi/extensions-source/tree/main/lib/speedbinb
mod pipeline;
use anyhow::{Context, Result, ensure};
pub use pipeline::{Episode, Pipeline};
use scraper::{Html, Selector};

fn selector(s: &str) -> Selector {
    Selector::parse(s).expect("static selector")
}
fn validate_origin(raw: &str, host: &str) -> Result<url::Url> {
    let url = url::Url::parse(raw)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some(host)
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "invalid YanMaga origin"
    );
    Ok(url)
}

pub fn decode_table(cid: &str, key: &str, encrypted: &str) -> Result<Vec<String>> {
    ensure!(
        cid.is_ascii()
            && key.is_ascii()
            && encrypted.is_ascii()
            && cid.len() <= 1024
            && key.len() <= 64
            && encrypted.len() <= 100_000,
        "invalid viewer table"
    );
    let mut state = format!("{cid}:{key}")
        .bytes()
        .enumerate()
        .fold(0u32, |s, (i, b)| s.wrapping_add((b as u32) << (i % 16)))
        & 0x7fff_ffff;
    if state == 0 {
        state = 0x12345678;
    }
    let mut decoded = String::new();
    for b in encrypted.bytes() {
        ensure!((32..=126).contains(&b), "invalid viewer table character");
        state = (state >> 1) ^ (1_210_056_708u32 & 0u32.wrapping_sub(state & 1));
        decoded.push(char::from((((b as u32 - 32) + state % 94) % 94 + 32) as u8));
    }
    let table: Vec<String> = serde_json::from_str(&decoded).context("invalid viewer table JSON")?;
    ensure!(table.len() == 8, "invalid viewer table length");
    Ok(table)
}

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
    ensure!(
        rw > 0 && rh > 0 && cw * ch <= 20_000_000,
        "invalid YanMaga image dimensions"
    );
    let mut output = image::RgbaImage::new(cw as u32, ch as u32);
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

fn viewer_key(cid: &str, entropy: &str) -> Result<String> {
    ensure!(
        !cid.is_empty()
            && cid.len() <= 128
            && cid.is_ascii()
            && entropy.len() >= 16
            && entropy.is_ascii(),
        "invalid YanMaga viewer ID"
    );
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let repeat = cid.repeat((16 + cid.len() - 1) / cid.len());
    let bytes = repeat.as_bytes();
    let (mut a, mut b, mut c) = (0usize, 0usize, 0usize);
    let mut key = String::new();
    for (i, ch) in entropy.bytes().take(16).enumerate() {
        ensure!(alphabet.contains(&ch), "invalid YanMaga viewer entropy");
        a ^= ch as usize;
        b ^= bytes[i] as usize;
        c ^= bytes[bytes.len() - 16 + i] as usize;
        key.push(ch as char);
        key.push(alphabet[(a + b + c) & 63] as char);
    }
    Ok(key)
}

pub struct ViewerPage {
    pub index: usize,
    url: url::Url,
    source: String,
    destination: String,
}

#[derive(Debug)]
pub struct ViewerAccessDenied;
impl std::fmt::Display for ViewerAccessDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("YanMaga viewer explicitly allows only a trial or denies access")
    }
}
impl std::error::Error for ViewerAccessDenied {}

pub fn viewer_item<'a>(json: &'a serde_json::Value, cid: &str) -> Result<&'a serde_json::Value> {
    ensure!(
        json["result"].as_i64() == Some(1),
        "YanMaga viewer response failed"
    );
    let items = json["items"]
        .as_array()
        .context("YanMaga viewer items missing")?;
    ensure!(items.len() == 1, "YanMaga viewer item mismatch");
    let item = &items[0];
    ensure!(
        item["ContentID"].as_str() == Some(cid),
        "YanMaga viewer content mismatch"
    );
    let mode = item["ViewMode"]
        .as_i64()
        .or_else(|| {
            item["ViewMode"]
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
        })
        .context("invalid YanMaga viewer mode")?;
    match mode {
        1 => Ok(item),
        -1 | 2 | 3 => Err(ViewerAccessDenied.into()),
        _ => anyhow::bail!("unknown YanMaga viewer mode"),
    }
}
fn anonymous_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(25))
        .user_agent("manga.rs/1.0")
        .build()?)
}
async fn reader_get(client: &reqwest::Client, url: url::Url, max: usize) -> Result<Vec<u8>> {
    let mut response = client
        .get(url)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", "https://yanmaga.jp/")
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("YanMaga reader request failed"))?;
    ensure!(
        response.status().is_success(),
        "YanMaga reader status {}",
        response.status()
    );
    ensure!(
        response.content_length().is_none_or(|n| n <= max as u64),
        "YanMaga reader response too large"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("YanMaga reader response interrupted"))?
    {
        ensure!(
            bytes.len().saturating_add(chunk.len()) <= max,
            "YanMaga reader response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn fetch_viewer(
    client: &reqwest::Client,
    episode_url: &str,
    attempt_id: &str,
) -> Result<Vec<ViewerPage>> {
    let episode = episode_url_checked(episode_url)?;
    let response = client
        .get(episode.clone())
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("YanMaga viewer request failed"))?;
    ensure!(
        response.status().is_redirection(),
        "YanMaga viewer did not redirect"
    );
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .context("YanMaga viewer redirect missing")?
        .to_str()?;
    let viewer = episode.join(location)?;
    ensure!(
        viewer.scheme() == "https"
            && viewer.host_str() == Some("yanmaga.jp")
            && viewer.port().is_none()
            && viewer.username().is_empty()
            && viewer.password().is_none()
            && viewer.path() == format!("/viewer{}", episode.path())
            && viewer.fragment().is_none(),
        "unsafe YanMaga viewer redirect"
    );
    let cid = viewer
        .query_pairs()
        .find(|(k, _)| k == "cid")
        .map(|(_, v)| v.into_owned())
        .context("YanMaga viewer ID missing")?;
    let key = viewer_key(&cid, attempt_id)?;
    let html = reader_get(client, viewer.clone(), 3_000_000).await?;
    let info_path = {
        let doc = Html::parse_document(std::str::from_utf8(&html)?);
        doc.select(&selector("#content[data-ptbinb]"))
            .next()
            .and_then(|e| e.value().attr("data-ptbinb"))
            .map(str::to_owned)
            .context("YanMaga viewer metadata missing")?
    };
    let mut info_url = viewer.join(&info_path)?;
    ensure!(
        info_url.scheme() == "https"
            && info_url.host_str() == Some("yanmaga.jp")
            && info_url.port().is_none()
            && info_url.username().is_empty()
            && info_url.password().is_none()
            && info_url.fragment().is_none()
            && info_url.path() == "/viewer/bibGetCntntInfo",
        "unsafe YanMaga viewer metadata URL"
    );
    ensure!(
        !info_url
            .query_pairs()
            .any(|(k, _)| matches!(k.as_ref(), "cid" | "k" | "dmytime")),
        "invalid YanMaga viewer parameters"
    );
    info_url
        .query_pairs_mut()
        .append_pair("cid", &cid)
        .append_pair("k", &key)
        .append_pair(
            "dmytime",
            &chrono::Utc::now().timestamp_millis().to_string(),
        );
    let info = reader_get(client, info_url, 3_000_000).await?;
    let json: serde_json::Value = serde_json::from_slice(&info)?;
    let item = viewer_item(&json, &cid)?;
    ensure!(
        item["ServerType"].as_i64() == Some(2) || item["ServerType"].as_str() == Some("2"),
        "unsupported YanMaga viewer server"
    );
    let pt = decode_table(
        &cid,
        &key,
        item["ptbl"]
            .as_str()
            .context("YanMaga tile table missing")?,
    )?;
    let ct = decode_table(
        &cid,
        &key,
        item["ctbl"]
            .as_str()
            .context("YanMaga tile table missing")?,
    )?;
    let base = validate_origin(
        item["ContentsServer"]
            .as_str()
            .context("YanMaga image server missing")?,
        "sbc.yanmaga.jp",
    )?;
    ensure!(
        base.query().is_none() && base.fragment().is_none(),
        "invalid YanMaga image base"
    );
    let base = if base.path().ends_with('/') {
        base
    } else {
        url::Url::parse(&format!("{base}/"))?
    };
    let content = reader_get(client, base.join("content")?, 3_000_000).await?;
    let metadata: serde_json::Value = serde_json::from_slice(&content)?;
    ensure!(
        metadata["result"].as_i64() == Some(1),
        "YanMaga content unavailable"
    );
    let ttx = metadata["ttx"].as_str().context("YanMaga pages missing")?;
    let case = ttx
        .split_once("<t-case")
        .context("YanMaga page group missing")?
        .1
        .split_once('>')
        .context("YanMaga page group invalid")?
        .1
        .split_once("</t-case>")
        .context("YanMaga page group incomplete")?
        .0;
    let pages = Html::parse_fragment(case);
    let mut result = Vec::new();
    for image in pages.select(&selector("t-img[src]")) {
        let src = image.value().attr("src").unwrap();
        ensure!(
            !src.is_empty()
                && src.len() <= 512
                && src
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-' | b'.'))
                && !src
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == ".."),
            "invalid YanMaga page path"
        );
        let name = src.rsplit('/').next().unwrap();
        let (mut even, mut odd) = (0usize, 0usize);
        for (i, b) in name.bytes().enumerate() {
            if i % 2 == 0 {
                even += b as usize
            } else {
                odd += b as usize
            }
        }
        let url = base.join(&format!("img/{src}"))?;
        ensure!(
            url.host_str() == Some("sbc.yanmaga.jp")
                && url.scheme() == "https"
                && url.path().starts_with(base.path()),
            "unsafe YanMaga image URL"
        );
        result.push(ViewerPage {
            index: result.len(),
            url,
            source: pt[even % 8].clone(),
            destination: ct[odd % 8].clone(),
        });
    }
    ensure!(
        !result.is_empty() && result.len() <= 200,
        "invalid YanMaga page count"
    );
    Ok(result)
}

/// Validate an episode URL without allowing other origins or viewer endpoints.
pub fn episode_url(url: &url::Url) -> Result<()> {
    validate_origin(url.as_str(), "yanmaga.jp")?;
    ensure!(
        url.query().is_none() && url.fragment().is_none(),
        "invalid YanMaga episode URL"
    );
    let segments: Vec<_> = url
        .path_segments()
        .context("invalid YanMaga episode path")?
        .collect();
    ensure!(
        segments.len() == 3
            && segments[0] == "comics"
            && !segments[1].is_empty()
            && segments[1].len() <= 990
            && !segments[1].to_ascii_lowercase().contains("%2f")
            && !segments[1].to_ascii_lowercase().contains("%5c"),
        "invalid YanMaga episode path"
    );
    ensure!(
        segments[2].len() == 32
            && segments[2]
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "invalid YanMaga episode ID"
    );
    Ok(())
}
fn episode_url_checked(raw: &str) -> Result<url::Url> {
    let url = url::Url::parse(raw)?;
    episode_url(&url)?;
    Ok(url)
}

fn solve_page(bytes: &[u8], page: &ViewerPage) -> Result<image::DynamicImage> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    ensure!(
        width > 0
            && height > 0
            && width <= 8192
            && height <= 8192
            && u64::from(width) * u64::from(height) <= 20_000_000,
        "invalid YanMaga image dimensions"
    );
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.to_rgba8();
    Ok(image::DynamicImage::ImageRgba8(restore_image(
        image,
        &page.source,
        &page.destination,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{data::MangaPage, pipeline::EpisodePipeline};
    #[tokio::test]
    async fn pipeline_decodes_an_unobfuscated_page() {
        let page = ViewerPage {
            index: 0,
            url: url::Url::parse("https://sbc.yanmaga.jp/test.png").unwrap(),
            source: String::new(),
            destination: String::new(),
        };
        assert_eq!(page.index().unwrap(), 0);
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([1, 2, 3, 255]),
        ));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let decoded = Pipeline::new()
            .unwrap()
            .solve_image(bytes.into_inner(), Some(page))
            .await
            .unwrap();
        assert_eq!(decoded.to_rgba8(), image.to_rgba8());
    }
}
