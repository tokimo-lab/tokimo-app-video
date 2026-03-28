use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rust_hls::{CreateSessionRequest, HlsSessionInfo};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::handlers::{err_resp, ok, ApiResponse};
use crate::scheduler::tasks::persist_playback_progress;
use crate::AppState;

/// POST /api/hls/sessions — create a new HLS transcoding session.
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<ApiResponse<HlsSessionInfo>>, (StatusCode, Json<ApiResponse<HlsSessionInfo>>)> {
    let port = std::env::var("PORT").unwrap_or_else(|_| "5678".to_string());
    let base_url = format!("http://127.0.0.1:{}", port);

    match state.hls_manager.create_session(req, &base_url).await {
        Ok(info) => Ok(ok(info)),
        Err(e) => Err(err_resp(StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

/// DELETE /api/hls/{session_id} — stop an HLS session.
pub async fn stop_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Response {
    debug!("[HLS] stop request for session {}", session_id);
    if let Some(snap) = state.hls_manager.stop_session(&session_id).await {
        if let Err(e) = persist_playback_progress(&state.db, &snap).await {
            warn!("[HLS] failed to persist final progress for {}: {}", session_id, e);
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

/// DELETE /api/hls/by-file/{file_id} — stop all HLS sessions for a file.
pub async fn stop_sessions_for_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
) -> Response {
    debug!("[HLS] stop-by-file request for file {}", file_id);
    // Get snapshots before stopping sessions
    let snapshots = state.hls_manager.playback_snapshots().await;
    let file_snapshots: Vec<_> = snapshots.into_iter().filter(|s| s.file_id == file_id).collect();
    state.hls_manager.stop_session_for_file(&file_id).await;
    for snap in &file_snapshots {
        if let Err(e) = persist_playback_progress(&state.db, snap).await {
            warn!("[HLS] failed to persist final progress for {}: {}", snap.session_id, e);
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

/// GET /api/hls/{session_id}/playlist.m3u8 — serve the VOD playlist.
pub async fn get_playlist(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Response {
    let session = match state.hls_manager.get_session(&session_id).await {
        Some(s) => s,
        None => {
            return err_resp::<()>(StatusCode::NOT_FOUND, "HLS session not found".into())
                .into_response()
        }
    };

    let playlist = {
        let s = session.lock().await;
        s.vod_playlist.clone()
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from(playlist))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// GET /api/hls/{session_id}/{segment} — serve an HLS segment file.
pub async fn get_segment(
    State(state): State<Arc<AppState>>,
    Path((session_id, segment)): Path<(String, String)>,
) -> Response {
    let req_start = std::time::Instant::now();

    let session = match state.hls_manager.get_session(&session_id).await {
        Some(s) => s,
        None => {
            return err_resp::<()>(StatusCode::NOT_FOUND, "HLS session not found".into())
                .into_response()
        }
    };

    // Phase 1: briefly lock the session to set up (seek-restart if needed).
    // Returns a SegmentWaitHandle with cloned Arcs — no lock held during the wait.
    let wait_handle = {
        let mut s = session.lock().await;
        s.prepare_segment_wait(&segment).await
    };

    let wait_handle = match wait_handle {
        Some(h) => h,
        None => {
            warn!("[HLS:{}] segment {} not available", session_id, segment);
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let prepare_ms = req_start.elapsed().as_millis();

    // Phase 2: wait for the segment WITHOUT holding the session lock.
    // This allows concurrent stop / seek requests to proceed normally.
    let segment_path = wait_handle.wait().await;

    let wait_ms = req_start.elapsed().as_millis();

    let segment_path = match segment_path {
        Some(path) => path,
        None => {
            warn!(
                "[HLS:{}] segment {} wait timeout (prepare={}ms wait={}ms)",
                session_id, segment, prepare_ms, wait_ms
            );
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    match tokio::fs::read(&segment_path).await {
        Ok(data) => {
            let total_ms = req_start.elapsed().as_millis();
            let size_kb = data.len() / 1024;
            debug!(
                "[HLS:{}] segment {} served: {}KB prepare={}ms wait={}ms total={}ms",
                session_id, segment, size_kb, prepare_ms, wait_ms, total_ms
            );
            // fMP4 segments (.tokimo/.m4s) and init segments (.mp4) use video/mp4;
            // legacy MPEG-TS (.ts) uses video/mp2t.
            let content_type = if segment.ends_with(".tokimo")
                || segment.ends_with(".m4s")
                || segment.ends_with(".mp4")
            {
                "video/mp4"
            } else {
                "video/mp2t"
            };
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CACHE_CONTROL, "no-cache")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(Body::from(data))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => {
            warn!(
                "[HLS:{}] failed to read segment {}: {}",
                session_id, segment, e
            );
            StatusCode::NOT_FOUND.into_response()
        }
    }
}
