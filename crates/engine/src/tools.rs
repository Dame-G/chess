//! External tools and data, downloaded once into a tools directory, each pinned to a version and a SHA-256 so every
//! machine tests with the same bits: fastchess v1.8.2 (the match runner) and the UHO opening book.
//!
//! Nothing downloads unless the caller calls [`setup`] (the `dameless setup` command). Everything else only looks at
//! what is installed.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// A pinned download: an archive, its hash, and the one file taken out of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asset {
    pub url: &'static str,
    pub sha256: &'static str,
    /// The file to take out of the archive.
    pub member: &'static str,
    /// Where it goes, under the tools directory.
    pub name: &'static str,
}

/// fastchess for this platform, when there is a release build of it.
#[cfg(all(windows, target_arch = "x86_64"))]
pub const FASTCHESS: Option<Asset> = Some(Asset {
    url: "https://github.com/Disservin/fastchess/releases/download/v1.8.2-alpha/fastchess-windows-x86-64.zip",
    sha256: "007ce550dc809510b34622a1aeb43fd443d40284045bef5c1bc39c8941114143",
    member: "fastchess-windows-x86-64/fastchess.exe",
    name: "fastchess-1.8.2.exe",
});

/// fastchess for this platform, when there is a release build of it.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub const FASTCHESS: Option<Asset> = Some(Asset {
    url: "https://github.com/Disservin/fastchess/releases/download/v1.8.2-alpha/fastchess-linux-x86-64.tar",
    sha256: "1003f920bebe841acdab5e6d7871e93171a4586857b3ec91c21ef5777f26ff96",
    member: "fastchess-linux-x86-64/fastchess",
    name: "fastchess-1.8.2",
});

/// fastchess for this platform, when there is a release build of it.
#[cfg(not(any(all(windows, target_arch = "x86_64"), all(target_os = "linux", target_arch = "x86_64"))))]
pub const FASTCHESS: Option<Asset> = None;

/// The book Reckless and Fishtest test with: unbalanced human openings, so fewer games end in draws.
pub const BOOK: Asset = Asset {
    url: "https://github.com/official-stockfish/books/raw/426eca422c202ef381a1380ada1873a7f185c4b9/UHO_Lichess_4852_v1.epd.zip",
    sha256: "4e298f11e8acfa106babe02968f2e61582145e7874c59284690b20b9650e0e07",
    member: "UHO_Lichess_4852_v1.epd",
    name: "UHO_Lichess_4852_v1.epd",
};

/// What is installed in a tools directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub fastchess: Option<PathBuf>,
    pub book: Option<PathBuf>,
}

impl Installed {
    pub fn complete(&self) -> bool {
        self.fastchess.is_some() && self.book.is_some()
    }
}

/// What is installed, without downloading anything.
pub fn installed(dir: &Path) -> Installed {
    let present = |a: &Asset| Some(dir.join(a.name)).filter(|p| p.is_file());
    Installed { fastchess: FASTCHESS.as_ref().and_then(present), book: present(&BOOK) }
}

/// Downloads and installs whatever is missing, checking each archive's SHA-256. Each step goes to `log`.
pub async fn setup(dir: &Path, log: &mut (dyn FnMut(&str) + Send)) -> Result<Installed> {
    let fastchess = FASTCHESS.context(
        "fastchess has no release build for this platform: build v1.8.2 yourself and put it in the tools directory",
    )?;
    let fastchess = ensure(dir, &fastchess, log).await?;
    let book = ensure(dir, &BOOK, log).await?;
    Ok(Installed { fastchess: Some(fastchess), book: Some(book) })
}

async fn ensure(dir: &Path, a: &Asset, log: &mut (dyn FnMut(&str) + Send)) -> Result<PathBuf> {
    let path = dir.join(a.name);
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(dir)?;
    log(&format!("downloading {}", a.url));
    let archive = dir.join(format!("{}.download", a.name));
    download(a.url, &archive).await?;
    let path = install(dir, a, &archive).await?;
    log(&format!("installed {}", path.display()));
    Ok(path)
}

/// Downloads a URL to a file.
async fn download(url: &str, to: &Path) -> Result<()> {
    let res = reqwest::get(url).await?.error_for_status().with_context(|| format!("download {url}"))?;
    let mut file = tokio::fs::File::create(to).await?;
    let mut body = res.bytes_stream();
    while let Some(chunk) = body.next().await {
        file.write_all(&chunk?).await?;
    }
    // Written out before the archive is opened again to be checked and extracted.
    file.flush().await?;
    Ok(())
}

/// Checks a downloaded archive against its pinned hash and takes its file out into the tools directory. The archive
/// is removed either way.
pub async fn install(dir: &Path, a: &Asset, archive: &Path) -> Result<PathBuf> {
    let checked = verify_sha256(archive, a.sha256);
    if let Err(e) = checked {
        let _ = std::fs::remove_file(archive);
        return Err(e.context(a.url));
    }
    let out = dir.join(a.name);
    let (archive, member, target) = (archive.to_owned(), a.member.to_owned(), out.clone());
    tokio::task::spawn_blocking(move || extract(&archive, &member, &target)).await??;
    Ok(out)
}

/// A file's SHA-256, in lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Fails unless a file has the expected SHA-256.
pub fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let got = sha256_file(path)?;
    if !got.eq_ignore_ascii_case(expected) {
        bail!("{} has SHA-256 {got}, expected {expected}", path.display());
    }
    Ok(())
}

/// Takes `member` out of a zip or tar archive into `out` (executable on Unix), then removes the archive.
fn extract(archive: &Path, member: &str, out: &Path) -> Result<()> {
    let partial = out.with_extension("partial");
    let mut dest = std::fs::File::create(&partial)?;
    let file = std::fs::File::open(archive)?;
    if zip_magic(archive) {
        let mut zip = zip::ZipArchive::new(file)?;
        let mut entry = zip.by_name(member).with_context(|| format!("{member} is not in the archive"))?;
        std::io::copy(&mut entry, &mut dest)?;
    } else {
        let mut tar = tar::Archive::new(file);
        // Taken by its file name wherever it sits (`./` or not), since the Linux archive's layout was never checked
        // on the Windows machine this was written on.
        let name = Path::new(member).file_name();
        let mut entry = tar
            .entries()?
            .filter_map(Result::ok)
            .find(|e| e.header().entry_type().is_file() && e.path().is_ok_and(|p| p.file_name() == name))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    fn tar_archive(path: &Path) {
        let mut tar = tar::Builder::new(std::fs::File::create(path).unwrap());
        for (name, body) in
            [("./fastchess-linux-x86-64/LICENSE", "license"), ("./fastchess-linux-x86-64/fastchess", "binary")]
        {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, name, body.as_bytes()).unwrap();
        }
        tar.finish().unwrap();
    }

    fn zip_archive(path: &Path) {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        zip.start_file("book/UHO.epd", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq -\n").unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn takes_a_member_out_of_a_tar_wherever_it_sits() {
        let dir = TempDir::new("tools-tar");
        let archive = dir.path().join("fastchess.download");
        tar_archive(&archive);
        let out = dir.path().join("fastchess-1.8.2");
        extract(&archive, "fastchess-linux-x86-64/fastchess", &out).unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "binary");
        assert!(!archive.exists());
    }

    #[tokio::test]
    async fn installs_an_archive_with_the_right_hash() {
        let dir = TempDir::new("tools-zip");
        let archive = dir.path().join("book.download");
        zip_archive(&archive);
        let asset = Asset {
            url: "https://example.invalid/book.zip",
            sha256: leak(sha256_file(&archive).unwrap()),
            member: "book/UHO.epd",
            name: "UHO.epd",
        };
        let out = install(dir.path(), &asset, &archive).await.unwrap();
        assert!(std::fs::read_to_string(out).unwrap().starts_with("rnbqkbnr"));
        assert!(!archive.exists());
    }

    #[tokio::test]
    async fn refuses_an_archive_with_the_wrong_hash() {
        let dir = TempDir::new("tools-bad");
        let archive = dir.path().join("book.download");
        zip_archive(&archive);
        let asset = Asset {
            url: "https://example.invalid/book.zip",
            sha256: BOOK.sha256,
            member: "book/UHO.epd",
            name: "UHO.epd",
        };
        let err = format!("{:#}", install(dir.path(), &asset, &archive).await.unwrap_err());
        assert!(err.contains("expected"), "{err}");
        assert!(!archive.exists() && !dir.path().join("UHO.epd").exists());
    }

    #[test]
    fn hashes_files() {
        let dir = TempDir::new("tools-hash");
        let file = dir.path().join("abc");
        std::fs::write(&file, b"abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_file(&file).unwrap(), abc);
        verify_sha256(&file, &abc.to_uppercase()).unwrap();
        assert!(verify_sha256(&file, BOOK.sha256).is_err());
    }

    #[test]
    fn says_what_is_installed() {
        let dir = TempDir::new("tools-installed");
        assert_eq!(installed(dir.path()), Installed { fastchess: None, book: None });
        std::fs::write(dir.path().join(BOOK.name), "x").unwrap();
        let i = installed(dir.path());
        assert_eq!(i.book, Some(dir.path().join(BOOK.name)));
        assert!(!i.complete());
    }
}
