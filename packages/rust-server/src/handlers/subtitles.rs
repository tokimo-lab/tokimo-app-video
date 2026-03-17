use std::sync::Arc;

use axum::{extract::State, Json};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rust_online_media_ingest::{
    models::{
        SubtitleDownloadRequest, SubtitleDownloadResponse, SubtitleSearchRequest,
        SubtitleSearchResult,
    },
    subtitles::{download_assrt_subtitle, search_assrt_subtitles},
};

use crate::{
    handlers::{err400, err500, ok, ApiResponse},
    AppState,
};

pub async fn search(
    State(_state): State<Arc<AppState>>,
    Json(input): Json<SubtitleSearchRequest>,
) -> Result<
    Json<ApiResponse<Vec<SubtitleSearchResult>>>,
    (
        axum::http::StatusCode,
        Json<ApiResponse<Vec<SubtitleSearchResult>>>,
    ),
> {
    if input.query.as_deref().unwrap_or("").trim().is_empty() {
        return Err(err400("请输入片名或文件名后再搜索 assrt 字幕".into()));
    }

    match search_assrt_subtitles(&input).await {
        Ok(results) => Ok(ok(results)),
        Err(message) => Err(err500(message)),
    }
}

pub async fn download(
    State(state): State<Arc<AppState>>,
    Json(input): Json<SubtitleDownloadRequest>,
) -> Result<
    Json<ApiResponse<SubtitleDownloadResponse>>,
    (
        axum::http::StatusCode,
        Json<ApiResponse<SubtitleDownloadResponse>>,
    ),
> {
    if input
        .download_path
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(err400("assrt 搜索结果缺少下载地址".into()));
    }

    match download_assrt_subtitle(&input, &state.online_media.staging_root).await {
        Ok(downloaded) => Ok(ok(SubtitleDownloadResponse {
            name: downloaded.name,
            format: downloaded.format,
            content_base64: STANDARD.encode(downloaded.content),
        })),
        Err(message) => Err(err500(message)),
    }
}
