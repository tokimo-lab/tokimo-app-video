use axum::{Json, http::StatusCode, response::{IntoResponse, Response}};
use serde::Serialize;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Unauthorized(String),
    BadRequest(String),
    Forbidden(String),
    Conflict(String),
    Internal(String),
    Gone(String),
    Database(sea_orm::DbErr),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Unauthorized(msg) => write!(f, "unauthorized: {msg}"),
            Self::BadRequest(msg) => write!(f, "bad request: {msg}"),
            Self::Forbidden(msg) => write!(f, "forbidden: {msg}"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::Internal(msg) => write!(f, "internal: {msg}"),
            Self::Gone(msg) => write!(f, "gone: {msg}"),
            Self::Database(err) => write!(f, "database: {err}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self { Self::Database(err) }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self { Self::Internal(format!("JSON error: {err}")) }
}

pub trait OptionExt<T> {
    fn not_found(self, msg: impl Into<String>) -> Result<T, AppError>;
    fn bad_request(self, msg: impl Into<String>) -> Result<T, AppError>;
    fn unauthorized(self, msg: impl Into<String>) -> Result<T, AppError>;
    fn internal(self, msg: impl Into<String>) -> Result<T, AppError>;
}

impl<T> OptionExt<T> for Option<T> {
    fn not_found(self, msg: impl Into<String>) -> Result<T, AppError> {
        self.ok_or_else(|| AppError::NotFound(msg.into()))
    }
    fn bad_request(self, msg: impl Into<String>) -> Result<T, AppError> {
        self.ok_or_else(|| AppError::BadRequest(msg.into()))
    }
    fn unauthorized(self, msg: impl Into<String>) -> Result<T, AppError> {
        self.ok_or_else(|| AppError::Unauthorized(msg.into()))
    }
    fn internal(self, msg: impl Into<String>) -> Result<T, AppError> {
        self.ok_or_else(|| AppError::Internal(msg.into()))
    }
}

#[derive(Serialize)]
struct ErrorBody { success: bool, error: String }

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            Self::Gone(msg) => (StatusCode::GONE, msg.clone()),
            Self::Database(err) => {
                tracing::error!("database error: {err}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal database error".to_string())
            }
        };
        (status, Json(ErrorBody { success: false, error: message })).into_response()
    }
}
