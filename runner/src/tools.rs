//! External tools and data the runner downloads once into the data directory, each pinned to a version and a
//! SHA-256 so every machine tests with the same bits: fastchess (the match runner) and the opening book.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use sha2::{Digest, Sha256};

struct Download {
    url: &'static str,
    sha256: &'static str,
    /// The file to take out of the archive.
    member: &'static str,
    /// Where it goes, under `tools/`.
    name: &'static str,
}

#[cfg(all(windows, target_arch = "x86_64"))]
const FASTCHESS: Download = Download {
    url: "https://github.com/Disservin/fastchess/releases/download/v1.8.2-alpha/fastchess-windows-x86-64.zip",
    sha256: "007ce550dc809510b34622a1aeb43fd443d40284045bef5c1bc39c8941114143",
    member: "fastchess-windows-x86-64/fastchess.exe",
    name: "fastchess-1.8.2.exe",
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const FASTCHESS: Download = Download {
    url: "https://github.com/Disservin/fastchess/releases/download/v1.8.2-alpha/fastchess-linux-x86-64.tar",
    sha256: "1003f920bebe841acdab5e6d7871e93171a4586857b3ec91c21ef5777f26ff96",
    member: "fastchess-linux-x86-64/fastchess",
    name: "fastchess-1.8.2",
};

/// The book Reckless and Fishtest test with: unbalanced human openings, so fewer games end in draws.
const BOOK: Download = Download {
    url: "https://github.com/official-stockfish/books/raw/426eca422c202ef381a1380ada1873a7f185c4b9/UHO_Lichess_4852_v1.epd.zip",
    sha256: "4e298f11e8acfa106babe02968f2e61582145e7874c59284690b20b9650e0e07",
    member: "UHO_Lichess_4852_v1.epd",
    name: "UHO_Lichess_4852_v1.epd",
};

pub async fn fastchess(data: &Path, log: &(dyn Fn(&str) + Sync)) -> Result<PathBuf> {
    ensure(data, &FASTCHESS, log).await
}

pub async fn book(data: &Path, log: &(dyn Fn(&str) + Sync)) -> Result<PathBuf> {
    ensure(data, &BOOK, log).await
}

async fn ensure(data: &Path, d: &Download, log: &(dyn Fn(&str) + Sync)) -> Result<PathBuf> {
    let dir = data.join("tools");
    let path = dir.join(d.name);
    if path.exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(&dir)?;
    log(&format!("downloading {}", d.url));
    let archive = dir.join(format!("{}.download", d.name));
    fetch(d.url, &archive, d.sha256).await?;
    let out = path.clone();
    let member = d.member.to_owned();
    tokio::task::spawn_blocking(move || extract(&archive, &member, &out)).await??;
    log(&format!("installed {}", path.display()));
    Ok(path)
}

/// Downloads to `to`, checking the SHA-256 as it streams.
async fn fetch(url: &str, to: &Path, sha256: &str) -> Result<()> {
    let res = reqwest::get(url).await?.error_for_status().with_context(|| format!("download {url}"))?;
    let mut file = std::fs::File::create(to)?;
    let mut hash = Sha256::new();
    let mut body = res.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        hash.update(&chunk);
        file.write_all(&chunk)?;
    }
    let got: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != sha256 {
        let _ = std::fs::remove_file(to);
        bail!("{url} has SHA-256 {got}, expected {sha256}");
    }
    Ok(())
}

fn extract(archive: &Path, member: &str, out: &Path) -> Result<()> {
    let partial = out.with_extension("partial");
    let mut dest = std::fs::File::create(&partial)?;
    let file = std::fs::File::open(archive)?;
    if archive.to_string_lossy().contains(".zip") || zip_magic(archive) {
        let mut zip = zip::ZipArchive::new(file)?;
        let mut entry = zip.by_name(member).with_context(|| format!("{member} is not in the archive"))?;
        std::io::copy(&mut entry, &mut dest)?;
    } else {
        let mut tar = tar::Archive::new(file);
        let mut entry = tar
            .entries()?
            .filter_map(Result::ok)
            .find(|e| e.path().is_ok_and(|p| p.to_string_lossy() == member))
            .with_context(|| format!("{member} is not in the archive"))?;
        std::io::copy(&mut entry, &mut dest)?;
    }
    drop(dest);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&partial, out)?;
    std::fs::remove_file(archive)?;
    Ok(())
}

fn zip_magic(path: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 4];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && magic == *b"PK\x03\x04"
}
