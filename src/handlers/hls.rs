use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use tokimo_package_hls::{CreateSessionRequest, HlsSessionInfo};
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use tracing::{debug, warn};

use crate::AppState;
use crate::handlers::{ApiResponse, err_resp, ok};

/// POST /api/hls/sessions — create a new HLS transcoding session.
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<ApiResponse<HlsSessionInfo>>, (StatusCode, Json<ApiResponse<HlsSessionInfo>>)> {
    let port = std::env::var("PORT").unwrap_or_else(|_| "5678".to_string());
    let base_url = format!("http://127.0.0.1:{port}");

    match state.hls_manager.create_session(req, &base_url).await {
        Ok(info) => Ok(ok(info)),
        Err(e) => Err(err_resp(StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

/// DELETE /`api/hls/{session_id`} — stop an HLS session.
pub async fn stop_session(State(state): State<Arc<AppState>>, Path(session_id): Path<String>) -> Response {
    debug!("[HLS] stop request for session {}", session_id);
    state.hls_manager.stop_session(&session_id).await;
    StatusCode::NO_CONTENT.into_response()
}

/// DELETE /api/hls/by-file/{file_id} — stop all HLS sessions for a file.
pub async fn stop_sessions_for_file(State(state): State<Arc<AppState>>, Path(file_id): Path<String>) -> Response {
    debug!("[HLS] stop-by-file request for file {}", file_id);
    state.hls_manager.stop_session_for_file(&file_id).await;
    StatusCode::NO_CONTENT.into_response()
}

/// GET /`api/hls/{session_id}/playlist.m3u8` — serve the VOD playlist.
pub async fn get_playlist(State(state): State<Arc<AppState>>, Path(session_id): Path<String>) -> Response {
    let Some(session) = state.hls_manager.get_session(&session_id).await else {
        return err_resp::<()>(StatusCode::NOT_FOUND, "HLS session not found".into()).into_response();
    };

    // Keep stream_sessions alive so cleanup_stale doesn't reap this file's session.
    if let Some(file_id) = state.hls_manager.get_file_id(&session_id).await {
        state.stream_sessions.touch(&file_id);
    }

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

/// GET /`api/hls/{session_id}/{segment`} — serve an HLS segment file.
pub async fn get_segment(
    State(state): State<Arc<AppState>>,
    Path((session_id, segment)): Path<(String, String)>,
) -> Response {
    let req_start = std::time::Instant::now();

    let Some(session) = state.hls_manager.get_session(&session_id).await else {
        return err_resp::<()>(StatusCode::NOT_FOUND, "HLS session not found".into()).into_response();
    };

    // Keep stream_sessions alive so cleanup_stale doesn't reap this file's session.
    if let Some(file_id) = state.hls_manager.get_file_id(&session_id).await {
        state.stream_sessions.touch(&file_id);
    }

    // Phase 1: briefly lock the session to set up (seek-restart if needed).
    // Returns a SegmentWaitHandle with cloned Arcs — no lock held during the wait.
    let wait_handle = {
        let mut s = session.lock().await;
        s.prepare_segment_wait(&segment).await
    };

    let Some(wait_handle) = wait_handle else {
        warn!("[HLS:{}] segment {} not available", session_id, segment);
        return StatusCode::NOT_FOUND.into_response();
    };

    let prepare_ms = req_start.elapsed().as_millis();

    // Phase 2: wait for the segment WITHOUT holding the session lock.
    // This allows concurrent stop / seek requests to proceed normally.
    let segment_path = wait_handle.wait().await;

    let wait_ms = req_start.elapsed().as_millis();

    let Some(segment_path) = segment_path else {
        warn!(
            "[HLS:{}] segment {} wait timeout (prepare={}ms wait={}ms)",
            session_id, segment, prepare_ms, wait_ms
        );
        return StatusCode::NOT_FOUND.into_response();
    };

    match tokio::fs::File::open(&segment_path).await {
        Ok(file) => {
            let meta = file.metadata().await.ok();
            let size = meta.as_ref().map(std::fs::Metadata::len);
            let total_ms = req_start.elapsed().as_millis();
            let size_kb = size.unwrap_or(0) / 1024;
            debug!(
                "[HLS:{}] segment {} served: {}KB prepare={}ms wait={}ms total={}ms",
                session_id, segment, size_kb, prepare_ms, wait_ms, total_ms
            );
            // fMP4 segments (.tokimo/.m4s) and init segments (.mp4) use video/mp4;
            // legacy MPEG-TS (.ts) uses video/mp2t.
            let content_type = if segment.ends_with(".tokimo")
                || std::path::Path::new(&segment)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("m4s"))
                || std::path::Path::new(&segment)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
            {
                "video/mp4"
            } else {
                "video/mp2t"
            };
            let stream = ReaderStream::new(file);
            let mut builder = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CACHE_CONTROL, "no-cache")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
            if let Some(len) = size {
                builder = builder.header(header::CONTENT_LENGTH, len);
            }
            builder
                .body(Body::from_stream(stream))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => {
            warn!("[HLS:{}] failed to read segment {}: {}", session_id, segment, e);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}
