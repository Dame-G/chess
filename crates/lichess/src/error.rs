//! What a call to Lichess can come to, beyond an answer. The token is never part of an error: errors are shown to
//! people and logged.

use serde_json::Value;

use crate::time::parse_iso;

/// A failed call.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// Lichess said no (a 4xx other than 429): 400 for a request it will not do, 401 for a wrong, revoked or expired
    /// token, 403 for a missing scope, 404 for something that does not exist.
    #[error("Lichess refused ({status}): {}", error.message)]
    Refused { status: u16, error: LichessError },
    /// HTTP 429. The client waits a full minute before its next request on this token, as Lichess asks.
    #[error("Lichess is rate-limiting this account: wait a minute")]
    RateLimited { error: LichessError },
    /// Lichess could not be reached, answered 5xx, or a stream broke or went silent.
    #[error("could not reach Lichess: {0}")]
    Unreachable(String),
    /// Lichess answered something this crate cannot read. On a stream it is one line, and the stream goes on.
    #[error("Lichess sent something unexpected: {0}")]
    Unexpected(String),
    /// The call needs a token and this client has none.
    #[error("this needs a Lichess login")]
    NoToken,
}

/// Lichess's error body: `{ "error": "..." }`, sometimes a map of field errors, OAuth's `error_description`, and for
/// rate limits `ratelimit: { key, seconds, until }`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LichessError {
    /// For people: Lichess's message, or the start of the body when it sent no JSON.
    pub message: String,
    pub ratelimit: Option<RateLimitInfo>,
}

/// The `ratelimit` block of an error.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RateLimitInfo {
    /// Which limit, such as `api.challenge`.
    pub key: Option<String>,
    /// Seconds until it lifts, when Lichess says.
    pub seconds: Option<f64>,
    /// When it lifts (ms since 1970), when Lichess says.
    pub until: Option<i64>,
}

impl LichessError {
    /// Reads an error body (any text).
    pub fn parse(text: &str) -> Self {
        let body = serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
        let message = match (&body["error_description"], &body["error"]) {
            (Value::String(s), _) => s.clone(),
            (_, Value::String(s)) => s.clone(),
            (_, Value::Null) => text.trim().chars().take(200).collect(),
            (_, other) => other.to_string(),
        };
        let limit = &body["ratelimit"];
        let ratelimit = limit.is_object().then(|| RateLimitInfo {
            key: limit["key"].as_str().map(str::to_owned),
            seconds: limit["seconds"].as_f64().filter(|s| *s >= 0.0),
            until: match &limit["until"] {
                Value::String(s) => parse_iso(s),
                Value::Number(n) => n.as_i64(),
                _ => None,
            },
        });
        Self { message, ratelimit }
    }
}

impl Error {
    /// The HTTP status of an answer Lichess gave, if it gave one.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Refused { status, .. } => Some(*status),
            Self::RateLimited { .. } => Some(429),
            _ => None,
        }
    }

    /// Lichess's error body, if it answered with one.
    pub fn lichess(&self) -> Option<&LichessError> {
        match self {
            Self::Refused { error, .. } | Self::RateLimited { error } => Some(error),
            _ => None,
        }
    }

    /// A 401: the token is wrong, revoked or expired.
    pub fn is_unauthorized(&self) -> bool {
        self.status() == Some(401)
    }

    /// The error for an answer with this status and body.
    pub(crate) fn from_answer(status: u16, text: &str) -> Self {
        match status {
            429 => Self::RateLimited { error: LichessError::parse(text) },
            400..=499 => Self::Refused { status, error: LichessError::parse(text) },
            _ => Self::Unreachable(format!("Lichess answered {status}")),
        }
    }

    /// A transport error, without the URL: it is harmless, but the error is shown to people and reads better short.
    pub(crate) fn transport(e: reqwest::Error) -> Self {
        Self::Unreachable(e.without_url().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lichess_errors() {
        let e = Error::from_answer(400, r#"{"error":{"clock.limit":["bad"]}}"#);
        assert_eq!(e.status(), Some(400));
        assert!(e.lichess().unwrap().message.contains("clock.limit"));
        let e = Error::from_answer(400, r#"{"error":"invalid_grant","error_description":"code expired"}"#);
        assert_eq!(e.lichess().unwrap().message, "code expired");
        let e = Error::from_answer(404, "Not found");
        assert_eq!(e.to_string(), "Lichess refused (404): Not found");
        assert!(Error::from_answer(401, r#"{"error":"No such token"}"#).is_unauthorized());
        assert!(matches!(Error::from_answer(503, "down"), Error::Unreachable(_)));
        let e = Error::from_answer(429, r#"{"error":"Too many requests","ratelimit":{"key":"api.x","seconds":5}}"#);
        assert!(matches!(&e, Error::RateLimited { .. }));
        assert_eq!(e.lichess().unwrap().ratelimit.as_ref().unwrap().seconds, Some(5.0));
    }
}
