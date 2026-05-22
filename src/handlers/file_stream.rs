use axum::{
    extract::{Path, Query, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    AppState,
    db::repos::media::file_repo::VideoFileRepo,
    db::repos::subtitle_repo::SubtitleRepo,
    handlers::media::stream::stream_driver_file,
    handlers::{err_resp, err404, err500},
};

use tokimo_package_subtitle::{
    resolve::{extract_start_time_ms, resolve_subtitle_tracks},
    tap_builder::build_stream_tap,
};

const INTERNAL_STREAM_ACCESS_HEADER: &str = "x-internal-stream-access-token";
const TOKIMO_USER_ID_HEADER: &str = "x-tokimo-user-id";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamAccessQuery {
    access_token: Option<String>,
    probe_only: Option<bool>,
}

/// Stream a media file to the HTTP client.
///
/// Auth priority:
/// 1. `x-tokimo-user-id` header — injected by the main server's data-plane proxy
///    for every already-authenticated request.  Presence means the main server has
///    already validated the session; we trust it unconditionally.
/// 2. `SESSION_ID` cookie — validated via the auth bus service (cached 30 s).
/// 3. `access_token` query param or `x-internal-stream-access-token` header —
///    validated against the system config internal stream token (cached 30 s).
pub async fn stream_media_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    Query(query): Query<StreamAccessQuery>,
    request: Request,
) -> Response {
    let db = state.db.clone();
    let _user_id = match validate_stream_access(
        &state,
        request.headers(),
        query.access_token.as_deref(),
    )
    .await
    {
        Ok(uid) => uid,
        Err(err) => {
            return err_resp::<()>(StatusCode::UNAUTHORIZED, err).into_response();
        }
    };

    let target = match VideoFileRepo::load_stream_target(&db, &file_id).await {
        Ok(Some(target)) => target,
        Ok(None) => return err404::<()>("Media file not found".into()).into_response(),
        Err(err) => {
            return err500::<()>(format!("media file lookup failed: {err}")).into_response();
        }
    };

    // Build subtitle tap for embedded subtitles (needed for SSE streaming).
    let tap_tx = if query.probe_only.unwrap_or(false) {
        None
    } else {
        match SubtitleRepo::load_file_subtitles(&db, &file_id).await {
            Ok(rows) if !rows.is_empty() => {
                let ffprobe_raw = rows[0].ffprobe_raw.clone();
                let start_time_ms = extract_start_time_ms(&ffprobe_raw);
                let subs: Vec<_> = rows
                    .iter()
                    .map(crate::db::models::subtitle::FileSubtitleRow::to_embedded_record)
                    .collect();
                let tracks = resolve_subtitle_tracks(&ffprobe_raw, &subs);
                build_stream_tap(
                    &state.subtitle_cache,
                    &state.tap_registry,
                    tracks,
                    &target.path,
                    &file_id,
                    start_time_ms,
                )
            }
            _ => None,
        }
    };

    // Register (or reuse) a cancellation token for this file's stream session.
    let cancel = state.stream_sessions.create_or_get(&file_id);
    tracing::debug!(
        "[StreamSession] DirectPlay session attached file_id={} path={}",
        file_id,
        target.path
    );

    let Some(source_id) = target.source_id.as_deref() else {
        return err500::<()>("Media file has no source_id".into()).into_response();
    };

    let vfs = match state.sources.ensure_vfs(source_id).await {
        Ok(vfs) => vfs,
        Err(err) => return err404::<()>(err).into_response(),
    };

    stream_driver_file(vfs, target.path, request.headers().clone(), tap_tx, cancel).await
}

fn session_id_from_cookie(cookie_header: Option<&axum::http::HeaderValue>) -> Option<String> {
    let cookie_header = cookie_header?.to_str().ok()?;
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|cookie| cookie.strip_prefix("SESSION_ID=").map(ToOwned::to_owned))
}

/// Validate stream access and return the authenticated `user_id` (if available).
async fn validate_stream_access(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    access_token: Option<&str>,
) -> Result<Option<String>, String> {
    // 1. Trust x-tokimo-user-id injected by the main server proxy.
    if let Some(user_id) = headers.get(TOKIMO_USER_ID_HEADER).and_then(|v| v.to_str().ok()) {
        return Ok(Some(user_id.to_string()));
    }

    // 2. Validate SESSION_ID cookie via bus auth service.
    if let Some(sid) = session_id_from_cookie(headers.get(header::COOKIE)) {
        if let Some(user_id) = state.auth_client.validate_session(&sid).await {
            return Ok(Some(user_id.to_string()));
        }
    }

    // 3. Validate internal stream token (query param or header).
    if let Some(token) = access_token {
        if state.auth_client.validate_internal_stream_token(token).await {
            return Ok(None);
        }
    }

    if let Some(token) = headers
        .get(INTERNAL_STREAM_ACCESS_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        if state.auth_client.validate_internal_stream_token(token).await {
            return Ok(None);
        }
    }

    Err("Unauthorized".into())
}
