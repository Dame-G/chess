//! Turning the adapters' errors into the shared vocabulary, so a route answers 400, 409, 429 or 502 without knowing
//! where the error came from.

use chess_core::Error;
use chess_lichess::Error as LichessError;

/// Lichess refused (400, or 404 for something it does not know), rate-limited us (429), could not be reached (502), or
/// the call needed a login we do not have (409).
pub(crate) fn lichess(e: LichessError) -> Error {
    match &e {
        LichessError::Refused { status: 404, .. } => Error::NotFound(e.to_string()),
        LichessError::Refused { .. } => Error::BadRequest(e.to_string()),
        LichessError::RateLimited { .. } => {
            Error::RateLimited { message: e.to_string(), retry_after_seconds: Some(60) }
        }
        LichessError::Unreachable(_) | LichessError::Unexpected(_) => Error::Upstream(e.to_string()),
        LichessError::NoToken => Error::Conflict("this needs a Lichess login".into()),
    }
}

/// An engine, a process or a file failed: nothing the caller can fix by asking differently.
pub(crate) fn internal(e: anyhow::Error) -> Error {
    Error::Internal(format!("{e:#}"))
}
