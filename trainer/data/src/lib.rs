//! Training data for Dameless's nets.
//!
//! - [`formats`]: viriformat (what bullet loads), Reckless's rbinpack, Stockfish's binpack (read and write),
//!   bulletformat (write) and fastchess PGN (read), all through one [`game::Game`].
//! - [`convert`], [`filter`], [`stats`], [`shuffle`]: the tools.
//! - [`rescore`]: new scores from an engine (or any [`rescore::Scorer`]).
//! - [`datagen`]: self-play with Reckless's `datagen` branch, resumable, capped.
//! - [`manifest`]: `manifest.json`, what the app's data registry reads.

pub mod convert;
pub mod datagen;
pub mod filter;
pub mod formats;
pub mod game;
pub mod manifest;
pub mod openings;
pub mod rescore;
pub mod rng;
pub mod shuffle;
pub mod stats;
pub mod uci;

/// Parses a size such as `10G`, `512M`, `64k` or `1000` (bytes; powers of 1024).
pub fn parse_size(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let (number, unit) = text.split_at(text.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(text.len()));
    let value: f64 = number.trim().parse().map_err(|_| format!("invalid size {text:?}"))?;
    let scale = match unit.trim().to_ascii_lowercase().trim_end_matches(['b', 'i']) {
        "" => 1u64,
        "k" => 1 << 10,
        "m" => 1 << 20,
        "g" => 1 << 30,
        "t" => 1 << 40,
        _ => return Err(format!("invalid size unit in {text:?}")),
    };
    Ok((value * scale as f64) as u64)
}

/// Parses a count such as `5M`, `200k` or `1000` (powers of 1000).
pub fn parse_count(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let (number, unit) = text.split_at(text.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(text.len()));
    let value: f64 = number.trim().parse().map_err(|_| format!("invalid count {text:?}"))?;
    let scale = match unit.trim().to_ascii_lowercase().as_str() {
        "" => 1f64,
        "k" => 1e3,
        "m" => 1e6,
        "g" | "b" => 1e9,
        _ => return Err(format!("invalid count unit in {text:?}")),
    };
    Ok((value * scale) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_counts() {
        assert_eq!(parse_size("10G").unwrap(), 10 << 30);
        assert_eq!(parse_size("512MiB").unwrap(), 512 << 20);
        assert_eq!(parse_size("1.5k").unwrap(), 1536);
        assert_eq!(parse_count("5M").unwrap(), 5_000_000);
        assert_eq!(parse_count("200k").unwrap(), 200_000);
        assert!(parse_size("3X").is_err());
    }
}
