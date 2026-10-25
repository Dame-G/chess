use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

/// The hosted UI. It may call a local runner with the token.
pub const HOSTED_ORIGIN: &str = "https://chess.dame.gg";

/// The Lichess the bot plays on, unless `LICHESS_URL` names another (a local mock in tests).
pub const LICHESS_URL: &str = "https://lichess.org";

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
    std::fs::write(&path, &token).with_context(|| format!("write {}", path.display()))?;
    Ok(token)
}
