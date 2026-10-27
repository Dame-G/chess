//! Where Lichess is and how patiently to talk to it.

use std::time::Duration;

/// Lichess itself.
pub const LICHESS_URL: &str = "https://lichess.org";

/// The servers a client talks to. Lichess's own by default; a mock in tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// lichess.org, or what `LICHESS_URL` says. No trailing slash.
    pub base: String,
    /// The opening explorer: explorer.lichess.org beside lichess.org, else `<base>/explorer`.
    pub explorer: String,
    /// The tablebase: tablebase.lichess.org beside lichess.org, else `<base>/tablebase`.
    pub tablebase: String,
    pub timing: Timing,
}

/// The waits Lichess asks for, and the ones we choose. Tests shorten them; nothing else should.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timing {
    /// After a 429, no request on that token before this has passed. Lichess asks for a full minute.
    pub rate_limit_pause: Duration,
    /// How long a plain request may take, answer included.
    pub request_timeout: Duration,
    /// How long a stream may stay silent before it counts as dead. Lichess sends a blank line every few seconds to
    /// keep its streams open, so a long silence means the connection is gone without having closed.
    pub stream_idle: Duration,
    /// A broken games export is reopened after this, times the breaks in a row.
    pub import_break: Duration,
    /// The player explorer keeps sending updates while it indexes; the last one within this long is the answer.
    pub explorer_wait: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            rate_limit_pause: Duration::from_secs(61),
            request_timeout: Duration::from_secs(20),
            stream_idle: Duration::from_secs(30),
            import_break: Duration::from_secs(5),
            explorer_wait: Duration::from_secs(8),
        }
    }
}

impl Config {
    /// Lichess at `base`, with the explorer and the tablebase beside it: Lichess's own servers for lichess.org,
    /// else under `base` (a mock serves them there).
    pub fn new(base: &str) -> Self {
        let base = base.trim().trim_end_matches('/').to_owned();
        let (explorer, tablebase) = if base == LICHESS_URL {
            ("https://explorer.lichess.org".to_owned(), "https://tablebase.lichess.org".to_owned())
        } else {
            (format!("{base}/explorer"), format!("{base}/tablebase"))
        };
        Self { base, explorer, tablebase, timing: Timing::default() }
    }

    /// From the environment: `LICHESS_URL` (default lichess.org), `LICHESS_EXPLORER_URL` and `LICHESS_TABLEBASE_URL`.
    pub fn from_env() -> Self {
        let var = |key: &str| {
            std::env::var(key).ok().map(|v| v.trim().trim_end_matches('/').to_owned()).filter(|v| !v.is_empty())
        };
        let mut config = Self::new(&var("LICHESS_URL").unwrap_or_else(|| LICHESS_URL.to_owned()));
        if let Some(explorer) = var("LICHESS_EXPLORER_URL") {
            config.explorer = explorer;
        }
        if let Some(tablebase) = var("LICHESS_TABLEBASE_URL") {
            config.tablebase = tablebase;
        }
        config
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new(LICHESS_URL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_servers_follow_the_base() {
        let real = Config::default();
        assert_eq!(real.explorer, "https://explorer.lichess.org");
        assert_eq!(real.tablebase, "https://tablebase.lichess.org");
        let mock = Config::new("http://127.0.0.1:4000/");
        assert_eq!(mock.base, "http://127.0.0.1:4000");
        assert_eq!(mock.explorer, "http://127.0.0.1:4000/explorer");
        assert_eq!(mock.tablebase, "http://127.0.0.1:4000/tablebase");
        assert_eq!(mock.timing.rate_limit_pause, Duration::from_secs(61));
    }
}
