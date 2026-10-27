//! The API error type: every handler returns `Result<_, AppError>`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use starbooru_core::ApiError;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("authentication required")]
    Unauthorized,
    #[error("you are not allowed to do that")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("{message}")]
    Conflict { code: &'static str, message: String },
    #[error("upload is larger than the server limit")]
    PayloadTooLarge,
    #[error("{0}")]
    Unsupported(String),
    /// A remote server failed during URL import.
    #[error("{0}")]
    Upstream(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn internal(e: impl std::fmt::Display) -> Self {
        AppError::Internal(e.to_string())
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Conflict { code, .. } => (StatusCode::CONFLICT, code),
            AppError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            AppError::Unsupported(_) => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported"),
            AppError::Upstream(_) => (StatusCode::BAD_GATEWAY, "fetch_failed"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        let message = if let AppError::Internal(detail) = &self {
            // Details go to the log, never to the client.
            error!("internal error: {detail}");
            "internal server error".to_owned()
        } else {
            self.to_string()
        };
        (
            status,
            Json(ApiError {
                code: code.to_owned(),
                message,
            }),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            sqlx::Error::Database(db) if db.is_unique_violation() => AppError::Conflict {
                code: "conflict",
                message: "that already exists".into(),
            },
            _ => AppError::internal(e),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::internal(e)
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        AppError::internal(format!("background task failed: {e}"))
    }
}

impl From<starbooru_core::tag::TagError> for AppError {
    fn from(e: starbooru_core::tag::TagError) -> Self {
        AppError::BadRequest(e.to_string())
    }
}

impl From<starbooru_core::query::QueryError> for AppError {
    fn from(e: starbooru_core::query::QueryError) -> Self {
        AppError::BadRequest(e.to_string())
    }
}
