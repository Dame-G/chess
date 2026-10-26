use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

/// The hosted UI. It may call a local runner with the token.
pub const HOSTED_ORIGIN: &str = "https://chess.dame.gg";

/// The Lichess the bot plays on, unless `LICHESS_URL` names another (a local mock in tests).
pub const LICHESS_URL: &str = "https://lichess.org";

/// The speed Reckless's OpenBench scales its clocks to: `bench` nodes per second, measured with every slot busy.
pub const REFERENCE_NPS: u64 = 660_000;

/// How tests and tunes scale their clocks to the machine's speed (see `engine::scale`). Read from the environment
/// when each test starts: `CHESS_REFERENCE_NPS` sets the reference speed, and `CHESS_SCALE_CLOCKS=off` plays the
/// clocks as written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scaling {
    pub enabled: bool,
    pub reference_nps: u64,
}

impl Scaling {
    pub fn from_env() -> Self {
        Self::parse(
            std::env::var("CHESS_SCALE_CLOCKS").ok().as_deref(),
            std::env::var("CHESS_REFERENCE_NPS").ok().as_deref(),
        )
    }

    /// On unless `enabled` is `off`, `0`, `false` or `no`; the reference is Reckless's unless a positive number.
    pub fn parse(enabled: Option<&str>, reference_nps: Option<&str>) -> Self {
        let off =
            enabled.is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "off" | "0" | "false" | "no"));
        let reference_nps =
            reference_nps.and_then(|v| v.trim().parse().ok()).filter(|n| *n > 0).unwrap_or(REFERENCE_NPS);
        Self { enabled: !off, reference_nps }
    }
}

pub struct Config {
    pub data: PathBuf,
    pub reckless: PathBuf,
    pub listen: SocketAddr,
    pub token: String,
    /// The Lichess server the bot talks to.
    pub lichess_url: String,
}

impl Config {
    pub fn load(data: PathBuf, reckless: PathBuf, listen: String, lichess_url: String) -> Result<Self> {
        std::fs::create_dir_all(&data).with_context(|| format!("create data directory {}", data.display()))?;
        let listen = listen.parse().with_context(|| format!("parse listen address {listen}"))?;
        // Absolute, because builds and matches run in other working directories: cargo in the build worktree.
        let data = std::path::absolute(&data).unwrap_or(data);
        let reckless = std::path::absolute(&reckless).unwrap_or(reckless);
        let token = load_or_create_token(&data)?;
        Ok(Self { data, reckless, listen, token, lichess_url })
    }

    /// Bound to a loopback address: requests without an `Origin` (the CLI, curl, agents) come from this machine.
    pub fn is_loopback(&self) -> bool {
        self.listen.ip().is_loopback()
    }
}

pub fn default_data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("chess-harness")
}

pub fn token_path(data: &Path) -> PathBuf {
    data.join("token")
}

/// The token is created once and kept in the data directory, so the CLI on the same machine can read it.
fn load_or_create_token(data: &Path) -> Result<String> {
    let path = token_path(data);
    if let Ok(token) = std::fs::read_to_string(&path) {
        let token = token.trim().to_owned();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("generate token: {e}"))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    write_private(&path, &token).with_context(|| format!("write {}", path.display()))?;
    Ok(token)
}

/// Writes a secret where only this user can read it: mode 0600 on Unix. On Windows the per-user data folder is
/// already private to the user.
pub fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::{
            io::Write,
            os::unix::fs::{OpenOptionsExt, PermissionsExt},
        };
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        // A file that was already there keeps its mode on open, so it is set again.
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(contents.as_bytes())
    }
    #[cfg(not(unix))]
    std::fs::write(path, contents)
}
