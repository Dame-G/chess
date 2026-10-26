//! HTTP to Lichess with the bot's token: JSON calls, form posts, and the NDJSON streams the Bot API pushes events on.
//! The token goes in the `Authorization` header and nowhere else, never into a URL, a log line or an error.

use std::time::Duration;

use anyhow::Result;
use serde_json::Value;

/// How long a stream may stay silent before it counts as dead. Lichess sends a blank line every few seconds to keep
/// its streams open, so a long silence means the connection is gone without having closed.
pub const STREAM_IDLE: Duration = Duration::from_secs(30);

/// What a call to Lichess can come to, beyond an answer.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// HTTP 429: Lichess asks for at least a minute's pause before anything else.
    #[error("Lichess is rate-limiting this account: wait a minute")]
    RateLimited,
    /// HTTP 401: the token is wrong, revoked or expired.
    #[error("Lichess rejected the token")]
    Unauthorized,
    #[error("Lichess answered {status}: {message}")]
    Status { status: u16, message: String },
    #[error("could not reach Lichess: {0}")]
    Network(String),
}

#[derive(Clone)]
pub struct Lichess {
    base: String,
    token: String,
    agent: String,
    http: reqwest::Client,
}

impl Lichess {
    /// `username` goes into the User-Agent once known, as Lichess asks of bots.
    pub fn new(http: reqwest::Client, base: &str, token: &str, username: Option<&str>) -> Self {
        let mut agent = format!("Dameless/{}", env!("CARGO_PKG_VERSION"));
        if let Some(name) = username {
            agent += &format!(" user:{name}");
        }
        Self { base: base.trim_end_matches('/').to_owned(), token: token.to_owned(), agent, http }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header(reqwest::header::USER_AGENT, &self.agent)
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response, CallError> {
        // Without the URL: it is harmless, but the error is shown to people and a short message reads better.
        let res = req.send().await.map_err(|e| CallError::Network(e.without_url().to_string()))?;
        let status = res.status();
        if status.is_success() {
            return Ok(res);
        }
        Err(match status.as_u16() {
            429 => CallError::RateLimited,
            401 => CallError::Unauthorized,
            code => {
                let text = res.text().await.unwrap_or_default();
                // Lichess answers errors as { "error": "..." }, sometimes with a map of field errors.
                let message = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| match &v["error"] {
                        Value::String(s) => Some(s.clone()),
                        Value::Null => None,
                        other => Some(other.to_string()),
                    })
                    .unwrap_or_else(|| text.chars().take(200).collect());
                CallError::Status { status: code, message }
            }
        })
    }

    /// A GET answered with JSON.
    pub async fn get(&self, path: &str) -> Result<Value, CallError> {
        let res = self.send(self.request(reqwest::Method::GET, path).timeout(Duration::from_secs(20))).await?;
        res.json().await.map_err(|e| CallError::Network(e.without_url().to_string()))
    }

    /// A POST with a form body (empty when `form` is), answered with JSON or nothing.
    pub async fn post(&self, path: &str, form: &[(&str, String)]) -> Result<Value, CallError> {
        let req = self.request(reqwest::Method::POST, path).timeout(Duration::from_secs(20)).form(form);
        let text = self.send(req).await?.text().await.map_err(|e| CallError::Network(e.without_url().to_string()))?;
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// A GET answered with a stream of JSON lines, such as `/api/stream/event`.
    pub async fn stream(&self, path: &str) -> Result<Ndjson, CallError> {
        let req = self.request(reqwest::Method::GET, path).header(reqwest::header::ACCEPT, "application/x-ndjson");
        Ok(Ndjson { res: self.send(req).await?, buffer: Vec::new(), idle: STREAM_IDLE })
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Streams of JSON lines
// ---------------------------------------------------------------------------------------------------------------------

/// One JSON document per line. Blank lines are Lichess's keep-alives and are skipped.
pub struct Ndjson {
    res: reqwest::Response,
    buffer: Vec<u8>,
    idle: Duration,
}

impl Ndjson {
    /// The next document, or `None` once the stream has ended. Cancel-safe: what has arrived stays buffered.
    pub async fn next(&mut self) -> Result<Option<Value>> {
        loop {
            while let Some(line) = take_line(&mut self.buffer) {
                if let Some(value) = parse_line(&line)? {
                    return Ok(Some(value));
                }
            }
            match tokio::time::timeout(self.idle, self.res.chunk()).await {
                Err(_) => anyhow::bail!("the stream went silent for {}s", self.idle.as_secs()),
                Ok(Err(e)) => anyhow::bail!("the stream broke: {}", e.without_url()),
                Ok(Ok(Some(chunk))) => self.buffer.extend_from_slice(&chunk),
                Ok(Ok(None)) => {
                    // A last line without its newline still counts.
                    let rest = String::from_utf8_lossy(&std::mem::take(&mut self.buffer)).into_owned();
                    return parse_line(&rest);
                }
            }
        }
    }
}

/// Takes the first whole line off the buffer, without its line ending.
pub fn take_line(buffer: &mut Vec<u8>) -> Option<String> {
    let end = buffer.iter().position(|b| *b == b'\n')?;
    let line: Vec<u8> = buffer.drain(..=end).collect();
    Some(String::from_utf8_lossy(&line).trim_end_matches(['\n', '\r']).to_owned())
}

/// A line of the stream: `None` for a keep-alive.
pub fn parse_line(line: &str) -> Result<Option<Value>> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(line).map_err(|e| anyhow::anyhow!("Lichess sent a line that is not JSON: {e}"))?))
}

/// Every document of a whole NDJSON body, such as the list of online bots.
pub fn parse_all(text: &str) -> Vec<Value> {
    text.lines().filter_map(|l| parse_line(l).ok().flatten()).collect()
}

impl Lichess {
    /// A GET answered with NDJSON that ends by itself, read whole.
    pub async fn get_lines(&self, path: &str) -> Result<Vec<Value>, CallError> {
        let req = self
            .request(reqwest::Method::GET, path)
            .header(reqwest::header::ACCEPT, "application/x-ndjson")
            .timeout(Duration::from_secs(30));
        let text = self.send(req).await?.text().await.map_err(|e| CallError::Network(e.without_url().to_string()))?;
        Ok(parse_all(&text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn keep_alives_are_skipped_and_garbage_is_an_error() {
        assert_eq!(parse_line("   ").unwrap(), None);
        assert_eq!(parse_line("{\"a\":1}").unwrap().unwrap()["a"], 1);
        assert!(parse_line("{nope").is_err());
        let all = parse_all("{\"id\":\"a\"}\n\n{\"id\":\"b\"}\n");
        assert_eq!(all.len(), 2);
        assert_eq!(all[1]["id"], "b");
    }
}
