//! The app's settings, read once at startup from the environment, and where everything lives in the data directory.

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use dameless_core::{ConnectionKey, WorkId, scaling::ScalingSettings};

/// The hosted UI. It may call a local app with the token.
pub const HOSTED_ORIGIN: &str = "https://chess.dame.gg";

/// Where the app listens unless `DAMELESS_LISTEN` says otherwise.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:7979";

/// The Lichess the adapters talk to unless `LICHESS_URL` names another (a mock in tests).
pub const LICHESS_URL: &str = "https://lichess.org";

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
    /// The Reckless fork's checkout, absolute: builds and git work happen beside it, never in it.
    pub reckless_dir: PathBuf,
    /// Cores the ledger hands out.
    pub cores: u32,
    pub scaling: ScalingSettings,
    pub lichess: LichessUrls,
    /// The hosted UI's origin, allowed with the token.
    pub hosted_origin: String,
}

impl Config {
    /// From the process's environment: `DAMELESS_DATA`, `DAMELESS_LISTEN`, `RECKLESS_DIR`, `DAMELESS_CORES`,
    /// `DAMELESS_SCALE_CLOCKS`, `DAMELESS_REFERENCE_NPS`, `LICHESS_URL`, `LICHESS_EXPLORER_URL`, `LICHESS_TABLEBASE_URL`.
    pub fn from_env() -> Result<Self> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// From any source of variables, so the rules can be tested without touching the environment. Empty values count
    /// as unset.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let var = |name: &str| var(name).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        let data_dir = var("DAMELESS_DATA").map(PathBuf::from).unwrap_or_else(default_data_dir);
        let listen = var("DAMELESS_LISTEN").unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
        let listen =
            listen.parse().with_context(|| format!("DAMELESS_LISTEN: {listen:?} is not an address and port"))?;
        let reckless_dir = var("RECKLESS_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("../reckless"));
        let cores = match var("DAMELESS_CORES") {
            Some(n) => n
                .parse::<u32>()
                .ok()
                .filter(|n| *n > 0)
                .with_context(|| format!("DAMELESS_CORES: {n:?} is not a positive number"))?,
            None => default_cores(),
        };
        let scaling =
            ScalingSettings::parse(var("DAMELESS_SCALE_CLOCKS").as_deref(), var("DAMELESS_REFERENCE_NPS").as_deref());
        let mut lichess = LichessUrls::new(&var("LICHESS_URL").unwrap_or_else(|| LICHESS_URL.to_owned()));
        if let Some(explorer) = var("LICHESS_EXPLORER_URL") {
            lichess.explorer = explorer.trim_end_matches('/').to_owned();
        }
        if let Some(tablebase) = var("LICHESS_TABLEBASE_URL") {
            lichess.tablebase = tablebase.trim_end_matches('/').to_owned();
        }
        let mut config = Self::new(data_dir, listen, reckless_dir, cores);
        config.scaling = scaling;
        config.lichess = lichess;
        Ok(config)
    }

    /// Explicit values, for tests and embedding; the rest at their defaults. Paths are made absolute, because builds
    /// and engines run in other working directories.
    pub fn new(data_dir: impl Into<PathBuf>, listen: SocketAddr, reckless_dir: impl Into<PathBuf>, cores: u32) -> Self {
        let absolute = |p: PathBuf| std::path::absolute(&p).unwrap_or(p);
        Self {
            data_dir: absolute(data_dir.into()),
            listen,
            reckless_dir: absolute(reckless_dir.into()),
            cores: cores.max(1),
            scaling: ScalingSettings::default(),
            lichess: LichessUrls::default(),
            hosted_origin: HOSTED_ORIGIN.to_owned(),
        }
    }

    /// Bound to a loopback address: requests without an `Origin` (the CLI, curl, agents) come from this machine.
    pub fn is_loopback(&self) -> bool {
        self.listen.ip().is_loopback()
    }

    /// Creates the data directory and its folders.
    pub fn create_dirs(&self) -> Result<()> {
        for dir in [
            self.data_dir.clone(),
            self.secrets_dir(),
            self.builds_dir(),
            self.games_dir(),
            self.tools_dir(),
            self.logs_dir(),
            self.worktrees_dir(),
        ] {
            std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        }
        Ok(())
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("dameless.db")
    }

    /// The API token, which the CLI on the same machine reads.
    pub fn token_path(&self) -> PathBuf {
        self.data_dir.join("token")
    }

    pub fn secrets_dir(&self) -> PathBuf {
        self.data_dir.join("secrets")
    }

    /// A Lichess connection's token: `secrets/lichess-you`, `secrets/lichess-bot`.
    pub fn lichess_token_path(&self, key: ConnectionKey) -> PathBuf {
        self.secrets_dir().join(format!("lichess-{key}"))
    }

    /// Built engine binaries.
    pub fn builds_dir(&self) -> PathBuf {
        self.data_dir.join("builds")
    }

    /// Archived PGN (zstd), such as trials' fastchess output.
    pub fn games_dir(&self) -> PathBuf {
        self.data_dir.join("games")
    }

    /// Downloaded tools: fastchess and opening books.
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

    /// Git worktrees of the engine checkout the app works in, so the user's checkout is never touched.
    pub fn worktrees_dir(&self) -> PathBuf {
        self.data_dir.join("worktrees")
    }

    /// The worktree builds happen in.
    pub fn build_worktree(&self) -> PathBuf {
        self.worktrees_dir().join("build")
    }

    /// The scratch worktree a promotion merges in.
    pub fn promote_worktree(&self) -> PathBuf {
        self.worktrees_dir().join("promote")
    }
}

/// `dameless` in the user's local data folder (`%LOCALAPPDATA%` on Windows).
pub fn default_data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("dameless")
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
        assert!(c.data_dir.ends_with("dameless"));
        assert!(c.reckless_dir.is_absolute() && c.reckless_dir.ends_with("reckless"));
        assert!(c.cores >= 1);
        assert_eq!(c.scaling, ScalingSettings::default());
        assert_eq!(c.lichess, LichessUrls::new("https://lichess.org/"));
        assert_eq!(c.lichess.explorer, "https://explorer.lichess.org");
        assert_eq!(c.hosted_origin, HOSTED_ORIGIN);
        assert!(c.is_loopback());
    }

    #[test]
    fn reads_the_environment() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::from_vars(vars(&[
            ("DAMELESS_DATA", dir.path().to_str().unwrap()),
            ("DAMELESS_LISTEN", "0.0.0.0:8000"),
            ("RECKLESS_DIR", "engine"),
            ("DAMELESS_CORES", "6"),
            ("DAMELESS_SCALE_CLOCKS", "off"),
            ("DAMELESS_REFERENCE_NPS", "500000"),
            ("LICHESS_URL", "http://127.0.0.1:9999/"),
            ("LICHESS_TABLEBASE_URL", "http://tb.test/"),
        ]))
        .unwrap();
        assert_eq!(c.data_dir, dir.path());
        assert!(!c.is_loopback());
        assert!(c.reckless_dir.is_absolute() && c.reckless_dir.ends_with("engine"));
        assert_eq!(c.cores, 6);
        assert_eq!(c.scaling, ScalingSettings { enabled: false, reference_nps: 500_000 });
        assert_eq!(c.lichess.url, "http://127.0.0.1:9999");
        assert_eq!(c.lichess.explorer, "http://127.0.0.1:9999/explorer");
        assert_eq!(c.lichess.tablebase, "http://tb.test");

        assert!(Config::from_vars(vars(&[("DAMELESS_CORES", "0")])).is_err());
        assert!(Config::from_vars(vars(&[("DAMELESS_LISTEN", "nowhere")])).is_err());
    }

    #[test]
    fn lays_out_the_data_directory() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::new(dir.path(), DEFAULT_LISTEN.parse().unwrap(), "reckless", 2);
        c.create_dirs().unwrap();
        assert_eq!(c.db_path(), dir.path().join("dameless.db"));
        assert_eq!(c.lichess_token_path(ConnectionKey::You), dir.path().join("secrets").join("lichess-you"));
        assert_eq!(c.lichess_token_path(ConnectionKey::Bot), dir.path().join("secrets").join("lichess-bot"));
        assert_eq!(c.work_log_path(WorkId(7)), dir.path().join("logs").join("work-7.log"));
        for d in ["secrets", "builds", "games", "tools", "logs", "worktrees"] {
            assert!(dir.path().join(d).is_dir(), "{d}");
        }
        assert_eq!(Config::new(dir.path(), DEFAULT_LISTEN.parse().unwrap(), "r", 0).cores, 1);
    }
}
