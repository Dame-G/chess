//! One error vocabulary for every crate: each error has an API code and an HTTP status, so a service deep in `work`
//! or `play` says "conflict" and the server answers 409 without translating.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The codes errors carry in the API's `{ error: { code, message } }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    NotFound,
    Conflict,
    /// Analysis asked for while you play on Lichess.
    FairPlay,
    RateLimited,
    /// Lichess (or another service) failed or could not be reached.
    Upstream,
    /// Something unexpected: a bug or a broken machine. Not in the design's list of codes, which names what callers
    /// can act on; every other error needs a home too.
    Internal,
}

impl ErrorCode {
    pub fn status(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Unauthorized => 401,
            Self::NotFound => 404,
            Self::Conflict | Self::FairPlay => 409,
            Self::RateLimited => 429,
            Self::Upstream => 502,
            Self::Internal => 500,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::FairPlay => "fair_play",
            Self::RateLimited => "rate_limited",
            Self::Upstream => "upstream",
            Self::Internal => "internal",
        }
    }
}

/// The sentence the fair-play policy refuses with.
pub const FAIR_PLAY_REASON: &str = "Analysis is off while you're playing on Lichess";

/// A domain error. Each variant holds a sentence for people.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    FairPlay(String),
    /// With how long to wait, when known.
    #[error("{message}")]
    RateLimited { message: String, retry_after_seconds: Option<u64> },
    #[error("{0}")]
    Upstream(String),
    #[error("{0}")]
    Internal(String),
}

impl Error {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        Self::Upstream(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// The fair-play refusal, with its one sentence.
    pub fn fair_play() -> Self {
        Self::FairPlay(FAIR_PLAY_REASON.into())
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            Self::BadRequest(_) => ErrorCode::BadRequest,
            Self::Unauthorized(_) => ErrorCode::Unauthorized,
            Self::NotFound(_) => ErrorCode::NotFound,
            Self::Conflict(_) => ErrorCode::Conflict,
            Self::FairPlay(_) => ErrorCode::FairPlay,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::Upstream(_) => ErrorCode::Upstream,
            Self::Internal(_) => ErrorCode::Internal,
        }
    }

    pub fn status(&self) -> u16 {
        self.code().status()
    }

    pub fn message(&self) -> String {
        self.to_string()
    }

    /// The API's body for this error.
    pub fn body(&self) -> ErrorBody {
        ErrorBody { error: ErrorDetail { code: self.code(), message: self.message() } }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The body of every error answer: `{ "error": { "code": "conflict", "message": "…" } }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ErrorDetail {
    pub code: ErrorCode,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_carry_their_code_and_status() {
        let e = Error::fair_play();
        assert_eq!((e.code(), e.status()), (ErrorCode::FairPlay, 409));
        assert_eq!(
            serde_json::to_value(e.body()).unwrap(),
            serde_json::json!({ "error": { "code": "fair_play", "message": FAIR_PLAY_REASON } })
        );
        assert_eq!(Error::RateLimited { message: "wait".into(), retry_after_seconds: Some(60) }.status(), 429);
        assert_eq!(Error::upstream("down").code().as_str(), "upstream");
    }
}
