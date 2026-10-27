//! Lichess's streams: one JSON document per line (NDJSON), with blank lines as keep-alives. And the backoff a caller
//! uses to reopen a stream that broke.

use std::{marker::PhantomData, time::Duration};

use serde::de::DeserializeOwned;

use crate::Error;

/// A stream of typed documents. Blank lines are Lichess's keep-alives and are skipped; a stream silent for longer than
/// its idle limit counts as broken.
pub struct Ndjson<T> {
    res: Option<reqwest::Response>,
    buffer: Vec<u8>,
    idle: Duration,
    _item: PhantomData<fn() -> T>,
}

impl<T: DeserializeOwned> Ndjson<T> {
    pub(crate) fn new(res: reqwest::Response, idle: Duration) -> Self {
        Self { res: Some(res), buffer: Vec::new(), idle, _item: PhantomData }
    }

    /// The next document, or `None` once the stream has ended. `Error::Unexpected` is one line that does not read as
    /// `T`, and the stream goes on; any other error means the stream is gone. Cancel-safe: what has arrived stays
    /// buffered, so this can sit in a `select!`.
    pub async fn next(&mut self) -> Result<Option<T>, Error> {
        loop {
            while let Some(line) = take_line(&mut self.buffer) {
                if let Some(item) = parse_line(&line)? {
                    return Ok(Some(item));
                }
            }
            let Some(res) = self.res.as_mut() else { return Ok(None) };
            match tokio::time::timeout(self.idle, res.chunk()).await {
                Err(_) => {
                    self.res = None;
                    return Err(Error::Unreachable(format!(
                        "the stream went silent for {} s",
                        self.idle.as_secs_f32().round()
                    )));
                }
                Ok(Err(e)) => {
                    self.res = None;
                    return Err(Error::Unreachable(format!("the stream broke: {}", e.without_url())));
                }
                Ok(Ok(Some(chunk))) => self.buffer.extend_from_slice(&chunk),
                Ok(Ok(None)) => {
                    // A last line without its newline still counts.
                    self.res = None;
                    let rest = String::from_utf8_lossy(&std::mem::take(&mut self.buffer)).into_owned();
                    return parse_line(&rest);
                }
            }
        }
    }
}

/// Takes the first whole line off the buffer, without its line ending.
pub(crate) fn take_line(buffer: &mut Vec<u8>) -> Option<String> {
    let end = buffer.iter().position(|b| *b == b'\n')?;
    let line: Vec<u8> = buffer.drain(..=end).collect();
    Some(String::from_utf8_lossy(&line).trim_end_matches(['\n', '\r']).to_owned())
}

/// A line of a stream: `None` for a keep-alive.
pub(crate) fn parse_line<T: DeserializeOwned>(line: &str) -> Result<Option<T>, Error> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(line).map(Some).map_err(|e| {
        let start: String = line.chars().take(120).collect();
        Error::Unexpected(format!("{e} in {start}"))
    })
}

/// Every document of a whole NDJSON body, such as the list of online bots. Lines that do not read are left out.
pub(crate) fn parse_all<T: DeserializeOwned>(text: &str) -> Vec<T> {
    text.lines().filter_map(|l| parse_line(l).ok().flatten()).collect()
}

/// How long to wait before reopening a stream that broke: a minute and a second after a 429 (Lichess asks for a
/// minute), else doubling from `min` to `max`. Reset it once a stream is open again.
#[derive(Clone, Debug)]
pub struct Backoff {
    min: Duration,
    max: Duration,
    next: Duration,
    rate_limited: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(Duration::from_secs(1), Duration::from_secs(60), Duration::from_secs(61))
    }
}

impl Backoff {
    pub fn new(min: Duration, max: Duration, rate_limited: Duration) -> Self {
        Self { min, max, next: min, rate_limited }
    }

    /// The wait before the next try after `error`.
    pub fn after(&mut self, error: &Error) -> Duration {
        let wait = if matches!(error, Error::RateLimited { .. }) { self.rate_limited } else { self.next };
        self.next = (self.next * 2).min(self.max);
        wait
    }

    /// The stream is open again: the next break starts from `min`.
    pub fn reset(&mut self) {
        self.next = self.min;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::LichessError;

    #[test]
    fn splits_lines_and_keeps_the_rest_for_later() {
        let mut buffer = b"{\"type\":\"gameStart\"}\n\n{\"type\":\"chal".to_vec();
        assert_eq!(take_line(&mut buffer).as_deref(), Some("{\"type\":\"gameStart\"}"));
        // The keep-alive is an empty line.
        assert_eq!(take_line(&mut buffer).as_deref(), Some(""));
        assert_eq!(take_line(&mut buffer), None);
        buffer.extend_from_slice(b"lenge\"}\r\n");
        assert_eq!(take_line(&mut buffer).as_deref(), Some("{\"type\":\"challenge\"}"));
        assert!(buffer.is_empty());
    }

    #[test]
    fn keep_alives_are_skipped_and_garbage_is_one_bad_line() {
        assert_eq!(parse_line::<Value>("   ").unwrap(), None);
        assert_eq!(parse_line::<Value>("{\"a\":1}").unwrap().unwrap()["a"], 1);
        assert!(matches!(parse_line::<Value>("{nope"), Err(Error::Unexpected(_))));
        let all: Vec<Value> = parse_all("{\"id\":\"a\"}\n\nnope\n{\"id\":\"b\"}\n");
        assert_eq!(all.len(), 2);
        assert_eq!(all[1]["id"], "b");
    }

    #[test]
    fn backs_off_and_waits_a_minute_after_a_429() {
        let mut b = Backoff::default();
        let broke = Error::Unreachable("gone".into());
        assert_eq!(b.after(&broke), Duration::from_secs(1));
        assert_eq!(b.after(&broke), Duration::from_secs(2));
        assert_eq!(b.after(&Error::RateLimited { error: LichessError::default() }), Duration::from_secs(61));
        for _ in 0..10 {
            b.after(&broke);
        }
        assert_eq!(b.after(&broke), Duration::from_secs(60));
        b.reset();
        assert_eq!(b.after(&broke), Duration::from_secs(1));
    }
}
