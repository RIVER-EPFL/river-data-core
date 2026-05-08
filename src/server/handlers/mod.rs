pub mod admin;
pub mod enroll;
pub mod events;
pub mod heartbeat;
pub mod commands;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug)]
pub enum SyncError {
    NotFound(String),
    Unauthorized(String),
    Forbidden(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for SyncError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

impl From<sea_orm::DbErr> for SyncError {
    fn from(e: sea_orm::DbErr) -> Self {
        Self::Internal(e.to_string())
    }
}

pub type SyncResult<T> = Result<T, SyncError>;
