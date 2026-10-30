//! Downloads for the built-in engines: GitHub release assets (with their sizes, read from the release's asset list),
//! plain files, and unpacking zip and tar archives into a directory.
//!
//! Nothing downloads unless the caller asks (the `setup` work, which `dameless setup` queues after showing the sizes).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

/// GitHub's REST API, where release asset lists come from.
pub const GITHUB_API: &str = "https://api.github.com";

/// What GitHub asks every API client to send.
const USER_AGENT: &str = concat!("dameless/", env!("CARGO_PKG_VERSION"));

/// One file of a GitHub release.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct Release {
    assets: Vec<ReleaseAsset>,
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().user_agent(USER_AGENT).build()?)
}

/// The assets of a release, by its tag: `release_assets(GITHUB_API, "official-stockfish/Stockfish", "sf_19")`.
pub async fn release_assets(api: &str, repo: &str, tag: &str) -> Result<Vec<ReleaseAsset>> {
    let url = format!("{}/repos/{repo}/releases/tags/{tag}", api.trim_end_matches('/'));
    let release: Release = client()?
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .with_context(|| format!("ask GitHub for {repo} {tag}"))?
        .error_for_status()
        .with_context(|| format!("ask GitHub for {repo} {tag}"))?
        .json()
        .await?;
    Ok(release.assets)
}

/// A file's size from a `HEAD` request (redirects followed), when the server says.
pub async fn content_length(url: &str) -> Option<u64> {
    let res = client().ok()?.head(url).send().await.ok()?.error_for_status().ok()?;
    res.headers().get(reqwest::header::CONTENT_LENGTH)?.to_str().ok()?.parse().ok()
}

/// Downloads a URL to a file (written beside it and renamed when complete), telling `progress` the bytes so far.
pub async fn download(url: &str, to: &Path, progress: &mut (dyn FnMut(u64) + Send)) -> Result<u64> {
    if let Some(dir) = to.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let res = client()?.get(url).send().await?.error_for_status().with_context(|| format!("download {url}"))?;
    let partial = to.with_extension("partial");
    let mut file = tokio::fs::File::create(&partial).await?;
    let mut body = res.bytes_stream();
    let mut done = 0u64;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.with_context(|| format!("download {url}"))?;
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        progress(done);
    }
    file.flush().await?;
    drop(file);
    tokio::fs::rename(&partial, to).await?;
    Ok(done)
}

/// Unpacks a zip or tar archive into `dir` (created if missing), keeping its layout, then removes the archive. Files on
/// Unix are made executable, since an engine is one of them.
pub async fn unpack(archive: &Path, dir: &Path) -> Result<()> {
    let (archive, dir) = (archive.to_owned(), dir.to_owned());
    tokio::task::spawn_blocking(move || unpack_blocking(&archive, &dir)).await?
}

fn unpack_blocking(archive: &Path, dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let file = std::fs::File::open(archive).with_context(|| format!("open {}", archive.display()))?;
    if zip_magic(archive) {
        let mut zip = zip::ZipArchive::new(file)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let Some(name) = entry.enclosed_name() else { bail!("{} has an unsafe path", archive.display()) };
            let out = dir.join(name);
            if entry.is_dir() {
                std::fs::create_dir_all(&out)?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut dest = std::fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut dest)?;
            executable(&out)?;
        }
    } else {
        let mut tar = tar::Archive::new(file);
        // `unpack_in` refuses entries that would land outside `dir`.
        for entry in tar.entries()? {
            let mut entry = entry?;
            entry.unpack_in(dir)?;
        }
        for path in files(dir) {
            executable(&path)?;
        }
    }
    std::fs::remove_file(archive)?;
    Ok(())
}

#[cfg(unix)]
fn executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn executable(_: &Path) -> Result<()> {
    Ok(())
}

fn zip_magic(path: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 4];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && magic == *b"PK\x03\x04"
}

/// Every file under a directory, at any depth.
pub fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The largest file under `dir` whose name `matches`: how an engine binary is found in an unpacked release, whatever
/// folder it sits in.
pub fn find_binary(dir: &Path, matches: impl Fn(&str) -> bool) -> Option<PathBuf> {
    files(dir)
        .into_iter()
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(&matches))
        .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn zip_archive(path: &Path) {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("stockfish/stockfish-windows-x86-64.exe", options).unwrap();
        zip.write_all(b"the engine, much larger than the readme").unwrap();
        zip.start_file("stockfish/README.md", options).unwrap();
        zip.write_all(b"readme").unwrap();
        zip.finish().unwrap();
    }

    fn tar_archive(path: &Path) {
        let mut tar = tar::Builder::new(std::fs::File::create(path).unwrap());
        for (name, body) in [("./stockfish/stockfish-ubuntu-x86-64", "binary"), ("./stockfish/AUTHORS", "a")] {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, body.as_bytes()).unwrap();
        }
        tar.finish().unwrap();
    }

    #[tokio::test]
    async fn unpacks_a_zip_and_finds_the_engine_in_it() {
        let dir = TempDir::new("tools-zip");
        let archive = dir.path().join("sf.zip");
        zip_archive(&archive);
        let out = dir.path().join("stockfish");
        unpack(&archive, &out).await.unwrap();
        assert!(!archive.exists());
        let found = find_binary(&out, |n| n.starts_with("stockfish")).unwrap();
        assert!(found.ends_with("stockfish/stockfish-windows-x86-64.exe"), "{}", found.display());
        assert_eq!(files(&out).len(), 2);
    }

    #[tokio::test]
    async fn unpacks_a_tar_wherever_its_files_sit() {
        let dir = TempDir::new("tools-tar");
        let archive = dir.path().join("sf.tar");
        tar_archive(&archive);
        let out = dir.path().join("stockfish");
        unpack(&archive, &out).await.unwrap();
        let found = find_binary(&out, |n| n.starts_with("stockfish")).unwrap();
        assert_eq!(std::fs::read_to_string(found).unwrap(), "binary");
    }

    #[test]
    fn reads_release_assets() {
        let release: Release = serde_json::from_str(
            r#"{ "tag_name": "sf_19", "assets": [{ "name": "a.zip", "size": 12, "browser_download_url": "https://x/a.zip" }] }"#,
        )
        .unwrap();
        assert_eq!(release.assets, [ReleaseAsset { name: "a.zip".into(), url: "https://x/a.zip".into(), size: 12 }]);
    }
}
