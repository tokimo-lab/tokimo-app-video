use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Serialize;
use tokimo_media_ingest::tooling::{
    resolve_ytdlp_binary_at, ytdlp_download_at, ytdlp_latest_version, ytdlp_version_at,
};
use ts_rs::TS;

use crate::{
    AppState,
    error::AppError,
    handlers::{ApiResponse, ok},
};

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct YtdlpStatusResponse {
    pub installed: bool,
    pub path: String,
    pub version: Option<String>,
    pub latest_version: Option<String>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct YtdlpUpdateResponse {
    pub version: String,
}

pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<ApiResponse<YtdlpStatusResponse>>, AppError> {
    let installed = resolve_ytdlp_binary_at(&state.ytdlp_root).is_some();
    let version = if installed {
        ytdlp_version_at(&state.ytdlp_root).await
    } else {
        None
    };
    let latest_version = ytdlp_latest_version().await.ok();

    Ok(ok(YtdlpStatusResponse {
        installed,
        path: state.ytdlp_root.display().to_string(),
        version,
        latest_version,
    }))
}

pub async fn update(State(state): State<Arc<AppState>>) -> Result<Json<ApiResponse<YtdlpUpdateResponse>>, AppError> {
    let latest_version = ytdlp_latest_version()
        .await
        .map_err(|err| AppError::Internal(format!("failed to resolve latest yt-dlp version: {err}")))?;

    let version = ytdlp_download_at(&latest_version, &state.ytdlp_root)
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to install yt-dlp: {err}")))?;

    Ok(ok(YtdlpUpdateResponse { version }))
}
