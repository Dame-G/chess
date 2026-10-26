//! Adapter: UCI processes, the engine pool, git and cargo builds, bench, fastchess.
//!
//! This crate knows the outside world of the engine (UCI, git, cargo, `reckless bench`, fastchess) and keeps its own
//! wire types for it: a UCI `info` line, a fastchess round's counts, a git branch. It depends on nothing else of ours;
//! `play`, `analysis` and `work` map its types onto `core`'s.
//!
//! Everything that runs a program goes through [`proc`], so cancelling (dropping) a build, a bench or a fastchess round
//! takes the whole process tree with it, on Windows (a Job Object) and on Unix (a process group) alike.

pub mod bench;
pub mod build;
pub mod fastchess;
pub mod git;
pub mod pool;
pub mod proc;
pub mod tools;
pub mod uci;

/// The error a cancelled operation returns, so callers can tell a cancel from a failure with [`is_cancelled`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// Whether an error is (or wraps) [`Cancelled`].
pub fn is_cancelled(error: &anyhow::Error) -> bool {
    error.chain().any(|e| e.is::<Cancelled>())
}

/// A program's file name on this platform: `reckless.exe` on Windows, `reckless` elsewhere.
pub fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::{Path, PathBuf};

    /// A fresh directory under the system's temp folder, removed when dropped.
    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(label: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("dameless-engine-{label}-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_a_cancel_from_a_failure() {
        assert!(is_cancelled(&anyhow::Error::new(Cancelled).context("building")));
        assert!(!is_cancelled(&anyhow::anyhow!("cargo failed")));
    }

    #[test]
    fn names_programs_for_the_platform() {
        assert!(exe("reckless").starts_with("reckless"));
        assert_eq!(exe("reckless").ends_with(".exe"), cfg!(windows));
    }
}
