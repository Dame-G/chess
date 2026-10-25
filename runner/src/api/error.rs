use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// An error as the API returns it: a status and `{ "error": "..." }`, the same shape the guard uses.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn not_found(message: &str) -> Self {
        Self { status: StatusCode::NOT_FOUND, message: message.to_owned() }
    }

    pub fn bad_request(message: &str) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: message.to_owned() }
    }

    pub fn conflict(message: &str) -> Self {
        Self { status: StatusCode::CONFLICT, message: message.to_owned() }
    }

    /// A service the runner relies on (Lichess) failed or could not be reached.
    pub fn bad_gateway(message: &str) -> Self {
        Self { status: StatusCode::BAD_GATEWAY, message: message.to_owned() }
    }

    /// A service the runner relies on (Lichess) asks for a pause.
    pub fn too_many_requests(message: &str) -> Self {
        Self { status: StatusCode::TOO_MANY_REQUESTS, message: message.to_owned() }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({ "error": self.message }))).into_response()
    }
}

/// Anything unexpected is a 500 with its message, so the CLI and the UI can show what went wrong.
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: format!("{e:#}") }
    }
}
