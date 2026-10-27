//! What a call to Lichess can come to, beyond an answer. The token is never part of an error: errors are shown to
//! people and logged.

use std::time::Duration;

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
    /// Which limit, such as `bot.vsBot.day`.
    pub key: Option<String>,
    /// Seconds until it lifts, when Lichess says.
    pub seconds: Option<f64>,
    /// When it lifts (ms since 1970), when Lichess says.
    pub until: Option<i64>,
}

/// Lichess's daily limit on games between bots (`bot.vsBot.day`, at most 100 in a rolling day), read from an error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BotDailyLimit {
    /// Our bot has played its games, so no game against any bot can start; else the bot challenged has played its own.
    pub ours: bool,
    /// How long until the limit lifts: `ratelimit.seconds`, else the time in `ratelimit.until` or in the message, else
    /// an hour.
    pub retry_after: Duration,
}

/// When the error says nothing of when its limit lifts.
const UNKNOWN_RESET: Duration = Duration::from_secs(3600);

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

    /// Whether this is Lichess's daily limit on games between bots, and whose, as of `now_ms` (ms since 1970).
    /// Lichess answers it with 429 or 400: "You played 100 games against other bots today, please wait before
    /// challenging another bot." (ours) or "<name> played 100 games against other bots today, please wait until
    /// <time> to challenge them." (theirs).
    pub fn bot_daily_limit(&self, now_ms: i64) -> Option<BotDailyLimit> {
        let error = self.lichess()?;
        let key = error.ratelimit.as_ref().and_then(|r| r.key.as_deref());
        if key != Some("bot.vsBot.day") && !error.message.contains("games against other bots") {
            return None;
        }
        let ours = !error.message.contains("challenge them");
        let limit = error.ratelimit.clone().unwrap_or_default();
        let retry_after = if let Some(s) = limit.seconds {
            Duration::from_secs_f64(s)
        } else {
            let until = limit.until.or_else(|| {
                error.message.split("until ").nth(1).and_then(|rest| parse_iso(rest.split_whitespace().next()?))
            });
            match until {
                Some(t) => Duration::from_millis((t - now_ms).max(0) as u64),
                None => UNKNOWN_RESET,
            }
        };
        Some(BotDailyLimit { ours, retry_after })
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
        assert_eq!(e.bot_daily_limit(0), None);
    }

    #[test]
    fn tells_the_daily_bot_limit_and_whose_it_is() {
        let ours = r#"{"error":"You played 100 games against other bots today, please wait before challenging another bot.","ratelimit":{"key":"bot.vsBot.day","seconds":3000}}"#;
        let limit = Error::from_answer(429, ours).bot_daily_limit(0).unwrap();
        assert!(limit.ours);
        assert_eq!(limit.retry_after.as_secs(), 3000);
        let theirs = r#"{"error":"Gamma played 100 games against other bots today, please wait until 1970-01-01T01:00:00Z to challenge them.","ratelimit":{"key":"bot.vsBot.day"}}"#;
        let limit = Error::from_answer(400, theirs).bot_daily_limit(0).unwrap();
        assert!(!limit.ours);
        assert_eq!(limit.retry_after.as_secs(), 3600);
        let until = r#"{"error":"You played 100 games against other bots today","ratelimit":{"key":"bot.vsBot.day","until":"1970-01-01T00:10:00Z"}}"#;
        assert_eq!(Error::from_answer(429, until).bot_daily_limit(60_000).unwrap().retry_after.as_secs(), 540);
        let vague = r#"{"error":"You played 100 games against other bots today"}"#;
        assert_eq!(Error::from_answer(400, vague).bot_daily_limit(0).unwrap().retry_after, UNKNOWN_RESET);
    }
}
