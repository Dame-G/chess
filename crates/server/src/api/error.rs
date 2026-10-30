//! Errors as the API answers them: `core`'s [`Error`] with its status and `{ "error": { "code", "message" } }`, and
//! the extractors routes use, whose rejections (a body or query that does not read) answer in the same shape.

use axum::{
    extract::{
        FromRequest, FromRequestParts,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chess_core::Error;

/// An error on its way out of a route.
#[derive(Debug)]
pub struct ApiError(pub Error);

pub type ApiResult<T> = Result<T, ApiError>;

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        Self(e)
    }
}

impl From<JsonRejection> for ApiError {
    fn from(r: JsonRejection) -> Self {
        Self(Error::bad_request(r.body_text()))
    }
}

impl From<QueryRejection> for ApiError {
    fn from(r: QueryRejection) -> Self {
        Self(Error::bad_request(r.body_text()))
    }
}

impl From<PathRejection> for ApiError {
    fn from(r: PathRejection) -> Self {
        Self(Error::bad_request(r.body_text()))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let e = self.0;
        let status = StatusCode::from_u16(e.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        if status.is_server_error() {
            tracing::error!("{e}");
        }
        let retry = match &e {
            Error::RateLimited { retry_after_seconds: Some(s), .. } => HeaderValue::from_str(&s.to_string()).ok(),
            _ => None,
        };
        let mut res = (status, axum::Json(e.body())).into_response();
        if let Some(v) = retry {
            res.headers_mut().insert(header::RETRY_AFTER, v);
        }
        res
    }
}

/// A JSON body; one that does not read is a 400 in the API's error shape.
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct Json<T>(pub T);

impl<T: serde::Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

/// A query string; one that does not read is a 400 in the API's error shape.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct Query<T>(pub T);

/// Path parameters; ones that do not read are a 400 in the API's error shape.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct Path<T>(pub T);

/// Text that must read as `T` (an id, a player key), or a 400 naming what it should have been.
pub fn parse<T: std::str::FromStr>(what: &str, text: &str) -> ApiResult<T> {
    text.trim().parse().map_err(|_| ApiError(Error::bad_request(format!("{text:?} is not {what}"))))
}

/// A comma-separated list (`blitz,rapid`), each item read by `read`; empty items are skipped.
pub fn list<T>(text: Option<&str>, what: &str, read: impl Fn(&str) -> Option<T>) -> ApiResult<Vec<T>> {
    text.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| read(s).ok_or_else(|| ApiError(Error::bad_request(format!("{s:?} is not {what}")))))
        .collect()
}
