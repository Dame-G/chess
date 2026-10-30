//! The app's settings, read once at startup from the environment, and where everything lives in the data directory.

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use chess_core::{ConnectionKey, WorkId};

/// The hosted UI. It may call a local app with the token.
pub const HOSTED_ORIGIN: &str = "https://chess.dame.gg";

/// Where the app listens unless `CHESS_LISTEN` says otherwise.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:7979";

/// The Lichess the adapters talk to unless `LICHESS_URL` names another (a mock in tests).
pub const LICHESS_URL: &str = "https://lichess.org";

/// GitHub's REST API, where the built-in engines' releases are listed.
pub const GITHUB_API: &str = "https://api.github.com";

/// The Lichess services the app talks to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LichessUrls {
    /// lichess.org, or what `LICHESS_URL` says. No trailing slash.
    pub url: String,
    /// The opening explorer: explorer.lichess.org beside lichess.org, else `<url>/explorer`.
    pub explorer: String,
    /// The tablebase: tablebase.lichess.org beside lichess.org, else `<url>/tablebase`.
    pub tablebase: String,
}

impl LichessUrls {
    /// The services of the Lichess at `url`: the real ones beside lichess.org, the mock's own paths beside a mock.
    pub fn new(url: &str) -> Self {
        let url = url.trim().trim_end_matches('/').to_owned();
        let (explorer, tablebase) = if url == LICHESS_URL {
            ("https://explorer.lichess.org".to_owned(), "https://tablebase.lichess.org".to_owned())
        } else {
            (format!("{url}/explorer"), format!("{url}/tablebase"))
        };
        Self { url, explorer, tablebase }
    }
}

impl Default for LichessUrls {
    fn default() -> Self {
        Self::new(LICHESS_URL)
    }
}

/// Everything the app is configured with.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// The data directory, absolute.
    pub data_dir: PathBuf,
    pub listen: SocketAddr,
    /// Cores the ledger hands out.
    pub cores: u32,
    pub lichess: LichessUrls,
    /// The hosted UI's origin, allowed with the token.
    pub hosted_origin: String,
    /// GitHub's API, where the built-in engines' release assets are listed (a mock in tests).
    pub github_api: String,
}

impl Config {
    /// From the process's environment: `CHESS_DATA`, `CHESS_LISTEN`, `CHESS_CORES`, `LICHESS_URL`,
    /// `LICHESS_EXPLORER_URL`, `LICHESS_TABLEBASE_URL`.
    pub fn from_env() -> Result<Self> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// From any source of variables, so the rules can be tested without touching the environment. Empty values count
    /// as unset.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let var = |name: &str| var(name).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        let data_dir = var("CHESS_DATA").map(PathBuf::from).unwrap_or_else(default_data_dir);
        let listen = var("CHESS_LISTEN").unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
        let listen = listen.parse().with_context(|| format!("CHESS_LISTEN: {listen:?} is not an address and port"))?;
        let cores = match var("CHESS_CORES") {
            Some(n) => n
                .parse::<u32>()
                .ok()
                .filter(|n| *n > 0)
                .with_context(|| format!("CHESS_CORES: {n:?} is not a positive number"))?,
            None => default_cores(),
        };
        let mut lichess = LichessUrls::new(&var("LICHESS_URL").unwrap_or_else(|| LICHESS_URL.to_owned()));
        if let Some(explorer) = var("LICHESS_EXPLORER_URL") {
            lichess.explorer = explorer.trim_end_matches('/').to_owned();
        }
        if let Some(tablebase) = var("LICHESS_TABLEBASE_URL") {
            lichess.tablebase = tablebase.trim_end_matches('/').to_owned();
        }
        let mut config = Self::new(data_dir, listen, cores);
        config.lichess = lichess;
        Ok(config)
    }

    /// Explicit values, for tests and embedding; the rest at their defaults. The data directory is made absolute,
    /// because engines run in other working directories.
    pub fn new(data_dir: impl Into<PathBuf>, listen: SocketAddr, cores: u32) -> Self {
        let data_dir = data_dir.into();
        Self {
            data_dir: std::path::absolute(&data_dir).unwrap_or(data_dir),
            listen,
            cores: cores.max(1),
            lichess: LichessUrls::default(),
            hosted_origin: HOSTED_ORIGIN.to_owned(),
            github_api: GITHUB_API.to_owned(),
        }
    }

    /// Bound to a loopback address: requests without an `Origin` (the CLI, curl, agents) come from this machine.
    pub fn is_loopback(&self) -> bool {
        self.listen.ip().is_loopback()
    }

    /// Creates the data directory and its folders.
    pub fn create_dirs(&self) -> Result<()> {
        for dir in [self.data_dir.clone(), self.secrets_dir(), self.games_dir(), self.tools_dir(), self.logs_dir()] {
            std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        }
        Ok(())
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("chess.db")
    }

    /// The API token, which the CLI on the same machine reads.
    pub fn token_path(&self) -> PathBuf {
        self.data_dir.join("token")
    }

    pub fn secrets_dir(&self) -> PathBuf {
        self.data_dir.join("secrets")
    }

    /// A Lichess connection's token: `secrets/lichess-you`.
    pub fn lichess_token_path(&self, key: ConnectionKey) -> PathBuf {
        self.secrets_dir().join(format!("lichess-{key}"))
    }

    /// Archived PGN (zstd).
    pub fn games_dir(&self) -> PathBuf {
        self.data_dir.join("games")
    }

    /// The built-in engines `chess setup` installs: `tools/stockfish/`, `tools/lc0/`, `tools/maia/`.
    pub fn tools_dir(&self) -> PathBuf {
        self.data_dir.join("tools")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    /// One work item's log: `logs/work-<id>.log`.
    pub fn work_log_path(&self, id: WorkId) -> PathBuf {
        self.logs_dir().join(format!("work-{id}.log"))
    }
}

/// `chess` in the user's local data folder (`%LOCALAPPDATA%` on Windows).
pub fn default_data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("chess")
}

/// Physical cores less one, so the machine stays usable; at least one.
pub fn default_cores() -> u32 {
    let physical = sysinfo::System::physical_core_count()
        .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
        .unwrap_or(1);
    (physical as u32).saturating_sub(1).max(1)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn defaults() {
        let c = Config::from_vars(vars(&[])).unwrap();
        assert_eq!(c.listen.to_string(), DEFAULT_LISTEN);
        assert!(c.data_dir.ends_with("chess"));
        assert!(c.cores >= 1);
        assert_eq!(c.lichess, LichessUrls::new("https://lichess.org/"));
        assert_eq!(c.lichess.explorer, "https://explorer.lichess.org");
        assert_eq!(c.hosted_origin, HOSTED_ORIGIN);
        assert_eq!(c.github_api, GITHUB_API);
        assert!(c.is_loopback());
    }

    #[test]
    fn reads_the_environment() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::from_vars(vars(&[
            ("CHESS_DATA", dir.path().to_str().unwrap()),
            ("CHESS_LISTEN", "0.0.0.0:8000"),
            ("CHESS_CORES", "6"),
            ("LICHESS_URL", "http://127.0.0.1:9999/"),
            ("LICHESS_TABLEBASE_URL", "http://tb.test/"),
        ]))
        .unwrap();
        assert_eq!(c.data_dir, dir.path());
        assert!(!c.is_loopback());
        assert_eq!(c.cores, 6);
        assert_eq!(c.lichess.url, "http://127.0.0.1:9999");
        assert_eq!(c.lichess.explorer, "http://127.0.0.1:9999/explorer");
        assert_eq!(c.lichess.tablebase, "http://tb.test");

        assert!(Config::from_vars(vars(&[("CHESS_CORES", "0")])).is_err());
        assert!(Config::from_vars(vars(&[("CHESS_LISTEN", "nowhere")])).is_err());
    }

    #[test]
    fn lays_out_the_data_directory() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::new(dir.path(), DEFAULT_LISTEN.parse().unwrap(), 2);
        c.create_dirs().unwrap();
        assert_eq!(c.db_path(), dir.path().join("chess.db"));
        assert_eq!(c.lichess_token_path(ConnectionKey::You), dir.path().join("secrets").join("lichess-you"));
        assert_eq!(c.work_log_path(WorkId(7)), dir.path().join("logs").join("work-7.log"));
        for d in ["secrets", "games", "tools", "logs"] {
            assert!(dir.path().join(d).is_dir(), "{d}");
        }
        assert_eq!(Config::new(dir.path(), DEFAULT_LISTEN.parse().unwrap(), 0).cores, 1);
    }
}
