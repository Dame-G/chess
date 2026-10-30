//! The app was called Dameless, and its data lived in a `dameless` folder. The first start of Chess with the default
//! data directory copies what is yours from that folder into the new one, and leaves the old folder as it was: it is
//! the backup.
//!
//! What is copied: the database (`dameless.db`, with its `-wal` and `-shm` files, renamed `chess.db…`), the API
//! `token`, your Lichess token (`secrets/lichess-you`), the PGN archive (`games/`) and the work logs (`logs/`). What is
//! not: `builds/`, `worktrees/`, `nets/`, `data/` and `tools/` (setup downloads the built-in engines again) and the
//! old bot's token. Engines in the copied database that pointed into the old `tools/` point into the new one.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The old data directory: `dameless` in the user's local data folder.
pub fn legacy_data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("dameless")
}

/// What a copy brought over, for the log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adopted {
    /// The old folder.
    pub from: PathBuf,
    /// Each item copied, as `old -> new` relative to the folders (`dameless.db -> chess.db`, `games/ (12 files)`).
    pub copied: Vec<String>,
    /// Engines in the database moved from the old `tools/` to the new one.
    pub engines_rebased: usize,
}

/// Copies the old data directory's own files into `new` when `new` does not exist and `old` does; `None` when there is
/// nothing to do. `old` is only read. A copy that fails part way removes what it made, so the next start tries again.
pub fn adopt(new: &Path, old: &Path) -> Result<Option<Adopted>> {
    if new.exists() || !old.is_dir() {
        return Ok(None);
    }
    let result = copy_into(new, old);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(new);
    }
    result.map(Some)
}

fn copy_into(new: &Path, old: &Path) -> Result<Adopted> {
    std::fs::create_dir_all(new).with_context(|| format!("create {}", new.display()))?;
    let mut adopted = Adopted { from: old.to_path_buf(), ..Adopted::default() };
    for suffix in ["", "-wal", "-shm"] {
        let (from, to) = (format!("dameless.db{suffix}"), format!("chess.db{suffix}"));
        if copy_file(&old.join(&from), &new.join(&to))? {
            adopted.copied.push(format!("{from} -> {to}"));
        }
    }
    for file in ["token", "secrets/lichess-you"] {
        if copy_file(&old.join(file), &new.join(file))? {
            adopted.copied.push(file.to_owned());
        }
    }
    for dir in ["games", "logs"] {
        let src = old.join(dir);
        if src.is_dir() {
            let n = copy_dir(&src, &new.join(dir))?;
            adopted.copied.push(format!("{dir}/ ({n} files)"));
        }
    }
    let db = new.join("chess.db");
    if db.is_file() {
        let tools = |dir: &Path| dir.join("tools").display().to_string();
        adopted.engines_rebased = chess_store::engines::rebase_engine_paths(&db, &tools(old), &tools(new))?;
    }
    Ok(adopted)
}

/// Copies one file when it exists (making its folder); whether it did.
fn copy_file(from: &Path, to: &Path) -> Result<bool> {
    if !from.is_file() {
        return Ok(false);
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::copy(from, to).with_context(|| format!("copy {} to {}", from.display(), to.display()))?;
    Ok(true)
}

/// Copies a folder and everything in it; how many files.
fn copy_dir(from: &Path, to: &Path) -> Result<usize> {
    std::fs::create_dir_all(to).with_context(|| format!("create {}", to.display()))?;
    let mut n = 0;
    for entry in std::fs::read_dir(from).with_context(|| format!("read {}", from.display()))? {
        let entry = entry?;
        let path = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            n += copy_dir(&path, &target)?;
        } else if copy_file(&path, &target)? {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {

    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[tokio::test]
    async fn copies_what_is_yours_and_leaves_the_old_folder() {
        let root = tempfile::tempdir().unwrap();
        let (old, new) = (root.path().join("dameless"), root.path().join("chess"));
        let old_tools = old.join("tools").display().to_string();
        // An old database as a plain file (a store would checkpoint it on its own thread while the test reads it).
        std::fs::create_dir_all(&old).unwrap();
        let conn = rusqlite::Connection::open(old.join("dameless.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE engines (id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, path TEXT NOT NULL,              args TEXT NOT NULL, options TEXT NOT NULL, id_name TEXT, nodes INTEGER, rating INTEGER,              created_at TEXT NOT NULL) STRICT",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO engines VALUES ('stockfish', 'Stockfish', 'builtin', ?1, '[]', '{}', NULL, NULL, NULL, 't')",
            [format!("{old_tools}/stockfish/stockfish")],
        )
        .unwrap();
        drop(conn);
        write(&old.join("token"), "t0k");
        write(&old.join("secrets/lichess-you"), "lip_you");
        write(&old.join("secrets/lichess-bot"), "lip_bot");
        write(&old.join("games/2026/1.pgn.zst"), "pgn");
        write(&old.join("logs/work-1.log"), "log");
        for skipped in ["builds/x", "worktrees/x", "nets/x", "data/x", "tools/stockfish/stockfish"] {
            write(&old.join(skipped), "x");
        }
        let before: Vec<_> = walk(&old);

        let adopted = adopt(&new, &old).unwrap().unwrap();
        assert_eq!(adopted.from, old);
        assert!(adopted.copied.contains(&"dameless.db -> chess.db".to_owned()), "{:?}", adopted.copied);
        assert!(adopted.copied.contains(&"secrets/lichess-you".to_owned()));
        assert!(adopted.copied.contains(&"games/ (1 files)".to_owned()));
        assert_eq!(adopted.engines_rebased, 1);

        assert!(new.join("chess.db").is_file());
        assert!(!new.join("dameless.db").exists());
        assert_eq!(std::fs::read_to_string(new.join("token")).unwrap(), "t0k");
        assert_eq!(std::fs::read_to_string(new.join("secrets/lichess-you")).unwrap(), "lip_you");
        assert_eq!(std::fs::read_to_string(new.join("games/2026/1.pgn.zst")).unwrap(), "pgn");
        assert!(new.join("logs/work-1.log").is_file());
        for skipped in ["secrets/lichess-bot", "builds", "worktrees", "nets", "data", "tools"] {
            assert!(!new.join(skipped).exists(), "{skipped}");
        }
        let conn = rusqlite::Connection::open(new.join("chess.db")).unwrap();
        let sf: String = conn.query_row("SELECT path FROM engines WHERE id = 'stockfish'", [], |r| r.get(0)).unwrap();
        drop(conn);
        assert!(Path::new(&sf).starts_with(new.join("tools")), "{sf}");

        // The old folder is the backup: nothing in it changed or went.
        assert_eq!(walk(&old), before);
        // Once the new folder exists, nothing is copied again.
        assert_eq!(adopt(&new, &old).unwrap(), None);
    }

    #[test]
    fn nothing_to_do_without_an_old_folder() {
        let root = tempfile::tempdir().unwrap();
        let new = root.path().join("chess");
        assert_eq!(adopt(&new, &root.path().join("dameless")).unwrap(), None);
        assert!(!new.exists());
    }

    /// Every file under `dir` with its contents, sorted.
    fn walk(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
        out.sort();
        out
    }
}
