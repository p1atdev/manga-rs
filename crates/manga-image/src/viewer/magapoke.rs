use anyhow::{Context, Result, ensure};

/// Reproduce the site's currently observed seed and 4x4 tile permutation.
/// Validated against the WebAssembly viewer's draw calls, not a guessed shuffle.
pub fn tile_permutation(title: u32, episode: u32, seed: &str) -> Result<[usize; 16]> {
    ensure!(
        title > 0 && episode > 0 && !seed.is_empty() && seed.len() <= 10,
        "invalid tile seed"
    );
    let alphabet = if title & 1 == 1 {
        b"q6jtf2xnog"
    } else {
        b"svdk0m7acl"
    };
    let mut decoded = 0u32;
    for ch in seed.bytes() {
        let digit = alphabet
            .iter()
            .position(|c| *c == ch)
            .context("unknown tile seed character")? as u32;
        decoded = decoded
            .checked_mul(10)
            .and_then(|n| n.checked_add(digit))
            .context("tile seed overflow")?;
    }
    let mut state = decoded ^ title.wrapping_add(episode);
    let mut order = [(0u32, 0usize); 16];
    for (idx, item) in order.iter_mut().enumerate() {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *item = (state, idx);
    }
    order.sort_by_key(|p| p.0);
    Ok(order.map(|p| p.1))
}
