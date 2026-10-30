//! Secrets: the API token and the Lichess connections' tokens, each a file in the data directory that only this user
//! can read. They are never rows of the database and no route returns them.

use std::{io, path::Path};

use dameless_core::ConnectionKey;

use crate::config::Config;

/// Reads, writes and deletes the secret files.
#[derive(Clone, Debug)]
pub struct Secrets {
    config: Config,
}

impl Secrets {
    pub fn new(config: &Config) -> Self {
        Self { config: config.clone() }
    }

    /// A Lichess connection's token, if one is stored.
    pub fn lichess_token(&self, key: ConnectionKey) -> io::Result<Option<String>> {
        read_secret(&self.config.lichess_token_path(key))
    }

    /// Stores a Lichess connection's token (mode 0600 on Unix).
    pub fn set_lichess_token(&self, key: ConnectionKey, token: &str) -> io::Result<()> {
        let path = self.config.lichess_token_path(key);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_private(&path, token.trim())
    }

    /// Forgets a Lichess connection's token; forgetting one that is not there is fine.
    pub fn delete_lichess_token(&self, key: ConnectionKey) -> io::Result<()> {
        match std::fs::remove_file(self.config.lichess_token_path(key)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// The API token: created once (24 random bytes, hex) and kept in the data directory, so the CLI on the same
    /// machine can read it.
    pub fn api_token(&self) -> io::Result<String> {
        let path = self.config.token_path();
        if let Some(token) = read_secret(&path)? {
            return Ok(token);
        }
        let mut bytes = [0u8; 24];
        getrandom::fill(&mut bytes).map_err(|e| io::Error::other(format!("generate a token: {e}")))?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_private(&path, &token)?;
        Ok(token)
    }
}

/// A secret file's contents, trimmed; none when the file is missing or blank.
fn read_secret(path: &Path) -> io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text.trim().to_owned()).filter(|t| !t.is_empty())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Writes a secret where only this user can read it: mode 0600 on Unix. On Windows the per-user data folder is already
/// private to the user.
pub fn write_private(path: &Path, contents: &str) -> io::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> (Secrets, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path(), crate::config::DEFAULT_LISTEN.parse().unwrap(), 1);
        (Secrets::new(&config), dir)
    }

    #[test]
    fn the_api_token_is_made_once() {
        let (s, dir) = secrets();
        let token = s.api_token().unwrap();
        assert_eq!(token.len(), 48);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(s.api_token().unwrap(), token);
        assert_eq!(std::fs::read_to_string(dir.path().join("token")).unwrap(), token);
    }

    #[test]
    fn lichess_tokens_are_files() {
        let (s, dir) = secrets();
        assert_eq!(s.lichess_token(ConnectionKey::You).unwrap(), None);
        s.set_lichess_token(ConnectionKey::You, " lip_you \n").unwrap();
        assert_eq!(s.lichess_token(ConnectionKey::You).unwrap().as_deref(), Some("lip_you"));
        assert!(dir.path().join("secrets").join("lichess-you").is_file());
        s.delete_lichess_token(ConnectionKey::You).unwrap();
        s.delete_lichess_token(ConnectionKey::You).unwrap();
        assert_eq!(s.lichess_token(ConnectionKey::You).unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn secrets_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (s, dir) = secrets();
        s.set_lichess_token(ConnectionKey::You, "lip_you").unwrap();
        let mode = std::fs::metadata(dir.path().join("secrets/lichess-you")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
