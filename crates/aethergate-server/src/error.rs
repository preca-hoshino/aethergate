//! Unified HTTP error types and JSON error responses for `aethergate-server`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Structured JSON error body returned by panel and Open API endpoints.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Machine-readable error code.
    pub code: &'static str,
    /// Human-readable error message.
    pub message: String,
}

/// Application error type mapped cleanly to HTTP status codes.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Client supplied invalid input or failed validation.
    #[error("{0}")]
    BadRequest(String),
    /// Request lacks valid authentication credentials.
    #[error("{0}")]
    Unauthorized(String),
    /// Authenticated caller lacks permission for this resource.
    #[error("{0}")]
    Forbidden(String),
    /// Requested resource does not exist.
    #[error("{0}")]
    NotFound(String),
    /// Resource conflict (e.g. overlapping ports or duplicate tag).
    #[error("{0}")]
    Conflict(String),
    /// Internal database, crypto, or upstream error.
    #[error("internal server error")]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound("record not found".to_owned()),
            sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
                Self::Conflict(format!("duplicate record: {}", db_err.message()))
            }
            other => Self::Internal(anyhow::Error::new(other)),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "unauthorized", msg),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, "forbidden", msg),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg),
            Self::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg),
            Self::Internal(err) => {
                tracing::error!(error = %err, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_owned(),
                )
            }
        };

        (status, Json(ErrorBody { code, message })).into_response()
    }
}

/// Convenience `Result` alias for Axum handlers.
pub type AppResult<T> = Result<T, AppError>;
