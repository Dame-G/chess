//! `manifest.json`: what a dataset directory holds, for the app's data registry.
//!
//! ```json
//! {
//!   "name": "dfrc-5k-1",
//!   "format": "viriformat",
//!   "positions": 1234567, "games": 15000, "size_bytes": 5432100,
//!   "source": "datagen",
//!   "score_units": "cp",
//!   "generator": { "tool": "data datagen", "config": { ... } },
//!   "created": "2026-10-26T12:00:00Z", "updated": "2026-10-26T13:00:00Z",
//!   "files": [ { "path": "part-000.vf", "format": "viriformat", "size_bytes": 2716050,
//!                "positions": 617000, "games": 7500, "sha256": "..." } ]
//! }
//! ```
//!
//! `format` is `mixed` when the files differ. Rebuilding keeps the name, source, generator and created time of the
//! manifest already there unless new ones are given, and reuses a file's counts when its size and hash are
//! unchanged.

use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::formats::{self, Format, pgn::ScoreUnits};

pub const MANIFEST_FILE: &str = "manifest.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    /// Relative to the directory, with `/`.
    pub path: String,
    pub format: Format,
    pub size_bytes: u64,
    pub positions: u64,
    pub games: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub name: String,
    /// The files' format, or `mixed`.
    pub format: String,
    pub positions: u64,
    pub games: u64,
    pub size_bytes: u64,
    /// Where the data came from: `datagen`, `pgn`, `import`, a URL, ...
    pub source: String,
    #[serde(default)]
    pub score_units: Option<ScoreUnits>,
    /// The settings that made it (datagen's config, the conversion's options), as the tool recorded them.
    #[serde(default)]
    pub generator: Option<serde_json::Value>,
    pub created: String,
    pub updated: String,
    pub files: Vec<FileEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct ManifestOptions {
    pub name: Option<String>,
    pub source: Option<String>,
    pub generator: Option<serde_json::Value>,
    pub score_units: Option<ScoreUnits>,
}

pub fn read(dir: &Path) -> Result<Option<Manifest>> {
    let path = dir.join(MANIFEST_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?))
}

/// Scans `dir` (and its subdirectories) for data files and builds its manifest.
pub fn build(dir: &Path, options: &ManifestOptions) -> Result<Manifest> {
    let previous = read(dir).ok().flatten();
    let mut paths = Vec::new();
    collect(dir, &mut paths)?;
    paths.sort();
    let mut files = Vec::new();
    for path in paths {
        let Some(format) = Format::from_path(&path) else { continue };
        let rel = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let size = std::fs::metadata(&path)?.len();
        let sha = sha256_file(&path)?;
        let reused = previous
            .as_ref()
            .and_then(|m| m.files.iter().find(|f| f.path == rel && f.size_bytes == size && f.sha256 == sha));
        let (positions, games) = match reused {
            Some(f) => (f.positions, f.games),
            None => formats::count(&path, format)?,
        };
        files.push(FileEntry { path: rel, format, size_bytes: size, positions, games, sha256: sha });
    }
    let format = match files.first() {
        Some(first) if files.iter().all(|f| f.format == first.format) => first.format.name().to_string(),
        Some(_) => "mixed".to_string(),
        None => "empty".to_string(),
    };
    let now = now_rfc3339();
    let default_name = dir
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "dataset".to_string());
    Ok(Manifest {
        name: options.name.clone().or_else(|| previous.as_ref().map(|m| m.name.clone())).unwrap_or(default_name),
        format,
        positions: files.iter().map(|f| f.positions).sum(),
        games: files.iter().map(|f| f.games).sum(),
        size_bytes: files.iter().map(|f| f.size_bytes).sum(),
        source: options
            .source
            .clone()
            .or_else(|| previous.as_ref().map(|m| m.source.clone()))
            .unwrap_or_else(|| "unknown".to_string()),
        score_units: options.score_units.or_else(|| previous.as_ref().and_then(|m| m.score_units)),
        generator: options.generator.clone().or_else(|| previous.as_ref().and_then(|m| m.generator.clone())),
        created: previous.as_ref().map(|m| m.created.clone()).unwrap_or_else(|| now.clone()),
        updated: now,
        files,
    })
}

pub fn build_and_write(dir: &Path, options: &ManifestOptions) -> Result<Manifest> {
    let manifest = build(dir, options)?;
    write_json_atomic(&dir.join(MANIFEST_FILE), &manifest)?;
    Ok(manifest)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut input = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Writes JSON to a temporary file beside `path`, then renames it over `path`.
pub fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(value)? + "\n")?;
    std::fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

/// The current UTC time as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    rfc3339(secs)
}

pub fn rfc3339(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let rem = unix % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_798_200_000), "2026-12-25T12:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }
}
