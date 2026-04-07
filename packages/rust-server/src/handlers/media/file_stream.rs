use axum::{
    extract::{ConnectInfo, Path, Query, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sea_orm::DatabaseConnection;
use std::{net::SocketAddr, sync::Arc};

use crate::{
    db::repos::{
        auth_repo::AuthRepo, media::file_repo::VideoFileRepo, subtitle_repo::SubtitleRepo,
    },
    handlers::media::stream::stream_driver_file,
    handlers::{err404, err500, err_resp},
    AppState,
};

use rust_subtitle::{
    resolve::{extract_start_time_ms, resolve_subtitle_tracks},
    tap_builder::build_stream_tap,
};

const INTERNAL_STREAM_ACCESS_HEADER: &str = "x-internal-stream-access-token";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamAccessQuery {
    access_token: Option<String>,
    probe_only: Option<bool>,
}

/// Stream a media file to the HTTP client.
///
/// All source types (local, SMB, SFTP, S3, cloud drives…) are served through the
/// unified VFS layer. There is no special case for `source_type == "local"` —
/// the local VFS driver handles range requests identically to remote drivers,
/// and this path must pass through `stream_driver_file` anyway to support the
/// subtitle tap and the session `CancellationToken`.
pub async fn stream_media_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    Query(query): Query<StreamAccessQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
) -> Response {
    let db = state.db.clone();
    let _user_id = match validate_stream_access(
        &db,
        request.headers().get(header::COOKIE),
        query.access_token.as_deref(),
        request.headers().get(INTERNAL_STREAM_ACCESS_HEADER),
        addr.ip(),
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
    // All spawned stream tasks select on this token and abort immediately when
    // the session is cancelled (browser close, explicit stop-session, etc.).
    let cancel = state.stream_sessions.create_or_get(&file_id);
    tracing::debug!("[StreamSession] DirectPlay session attached file_id={} path={}", file_id, target.path);

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
/// For loopback connections, also attempts to extract `user_id` from cookie.
async fn validate_stream_access(
    db: &DatabaseConnection,
    cookie_header: Option<&axum::http::HeaderValue>,
    access_token: Option<&str>,
    access_token_header: Option<&axum::http::HeaderValue>,
    client_ip: std::net::IpAddr,
) -> Result<Option<String>, String> {
    let user_id = if let Some(sid) = session_id_from_cookie(cookie_header) {
        AuthRepo::get_user_id_by_session(db, &sid).await.ok().flatten()
    } else {
        None
    };

    if client_ip.is_loopback() {
        return Ok(user_id);
    }

    if let Some(token) = access_token
        && AuthRepo::validate_internal_stream_token(db, token)
            .await
            .unwrap_or(false)
    {
        return Ok(user_id);
    }

    if let Some(token) = access_token_header.and_then(|value| value.to_str().ok())
        && AuthRepo::validate_internal_stream_token(db, token)
            .await
            .unwrap_or(false)
    {
        return Ok(user_id);
    }

    if user_id.is_some() {
        return Ok(user_id);
    }

    Err("Unauthorized".into())
}
