use std::path::Path;
use axum::response::Response;
use crate::AppState;
use crate::db::models::media::MediaFileStreamTarget;
use crate::error::AppError;

/// Stream a driver-local file to the client.
pub async fn stream_driver_file(
    _state: &AppState,
    _target: &MediaFileStreamTarget,
    _request_headers: &axum::http::HeaderMap,
) -> Result<Response, AppError> {
    Err(AppError::Internal("streaming not implemented".into()))
}
