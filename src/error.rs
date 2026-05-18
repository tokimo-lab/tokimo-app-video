//! 统一错误类型 — video binary 自己的 AppError，独立于主仓 `crate::error::AppError`。
//!
//! 设计：
//! - `AppError` 是 axum 错误响应单元（含 HTTP status + message）
//! - 自动从 `sea_orm::DbErr` / `anyhow::Error` / `reqwest::Error` / `std::io::Error` 转换
//! - 实现 `IntoResponse`，repo / handler 可以 `?` 直接传播
//!
//! 这与 helloworld 的 AppError 同款，预留 `OptionExt` 兼容主仓老接口。

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

pub struct AppError {
    pub status: StatusCode,
    pub message: String,
}

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: msg.into(),
        }
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: msg.into(),
        }
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: msg.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({ "error": self.message });
        (self.status, Json(body)).into_response()
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(e: sea_orm::DbErr) -> Self {
        Self::internal(format!("db: {e}"))
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::internal(format!("{e:#}"))
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        Self::internal(format!("http: {e}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::internal(format!("io: {e}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::internal(format!("json: {e}"))
    }
}

/// 兼容主仓 `crate::error::OptionExt`：`option.ok_or_not_found("msg")`。
pub trait OptionExt<T> {
    fn ok_or_not_found(self, msg: impl Into<String>) -> Result<T, AppError>;
    fn ok_or_bad_request(self, msg: impl Into<String>) -> Result<T, AppError>;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_or_not_found(self, msg: impl Into<String>) -> Result<T, AppError> {
        self.ok_or_else(|| AppError::not_found(msg))
    }
    fn ok_or_bad_request(self, msg: impl Into<String>) -> Result<T, AppError> {
        self.ok_or_else(|| AppError::bad_request(msg))
    }
}
