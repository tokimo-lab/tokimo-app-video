use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query, Request, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
};

use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};
use sqlx::PgPool;
use tower::util::ServiceExt;
use tower_http::services::ServeFile;
use tracing::{info, trace};

use crate::{
    db::repos::{auth_repo::AuthRepo, media_file_repo::MediaFileRepo, subtitle_repo::SubtitleRepo},
    handlers::media_stream::stream_driver_file,
    handlers::{err404, err500, err_resp},
    AppState,
};

use rust_subtitle::{
    resolve::{extract_start_time_ms, resolve_subtitle_tracks},
    tap_builder::build_stream_tap,
};

const LOCAL_MEDIA_STREAM_CHUNK_SIZE: usize = 1024 * 1024;
const REMOTE_FS_SOURCE_TYPES: [&str; 6] = ["smb", "nfs", "webdav", "ftp", "sftp", "s3"];
const INTERNAL_STREAM_ACCESS_HEADER: &str = "x-internal-stream-access-token";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamAccessQuery {
    access_token: Option<String>,
    probe_only: Option<bool>,
    // Tracking metadata (passed by Node.js stream-url handler for progress tracking)
    dp_user: Option<String>,
    dp_movie: Option<String>,
    dp_episode: Option<String>,
    dp_dur: Option<f64>,
    dp_size: Option<u64>,
}

// ── Subtitle event handlers (SubtitleCache state) ────────────────────────────

pub async fn get_subtitle_events(
    State(cache): State<rust_subtitle::SubtitleCache>,
    Path(subtitle_id): Path<String>,
    Query(query): Query<rust_subtitle::types::SubtitleEventsQuery>,
) -> Response {
    let start_ms = query.start_ms.unwrap_or(0.0) as i64;
    let end_ms = query.end_ms.unwrap_or(i64::MAX as f64) as i64;

    match cache.query(&subtitle_id, start_ms, end_ms) {
        Some((events, complete)) => {
            let body = serde_json::json!({ "events": events, "complete": complete });
            axum::Json(body).into_response()
        }
        None => {
            let body = serde_json::json!({ "events": [], "complete": false });
            axum::Json(body).into_response()
        }
    }
}

pub async fn subtitle_events_sse(
    State(cache): State<rust_subtitle::SubtitleCache>,
    Path(subtitle_id): Path<String>,
    Query(_query): Query<rust_subtitle::types::SubtitleEventsQuery>,
) -> axum::response::sse::Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
        let (snapshot, mut rx) = cache.subscribe(&subtitle_id);

    info!(
        "[SSE] subscriber connected for sub={}, snapshot={} events",
        subtitle_id,
        snapshot.len()
    );

    let sub_id = subtitle_id.clone();
    let stream = async_stream::stream! {
        for ev in &snapshot {
            let json = serde_json::to_string(ev).unwrap_or_default();
            if ev.data.is_some() {
                yield Ok::<_, std::convert::Infallible>(Event::default().event("pgs").data(json));
            } else {
                yield Ok::<_, std::convert::Infallible>(Event::default().data(json));
            }
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    trace!("[SSE] pushing event to sub={}: timeMs={}", sub_id, ev.time_ms);
                    let json = serde_json::to_string(&ev).unwrap_or_default();
                    if ev.data.is_some() {
                        yield Ok(Event::default().event("pgs").data(json));
                    } else {
                        yield Ok(Event::default().data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    info!("[SSE] subtitle {} lagged {} events", sub_id, n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("[SSE] broadcast closed for sub={}", sub_id);
                    break;
                }
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── Media stream handler ────────────────────────────────────────────────────

pub async fn stream_media_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    Query(query): Query<StreamAccessQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
) -> Response {
    let db = state.sources.db_pool();
    if let Err(err) = validate_stream_access(
        &db,
        request.headers().get(header::COOKIE),
        query.access_token.as_deref(),
        request.headers().get(INTERNAL_STREAM_ACCESS_HEADER),
        addr.ip(),
    )
    .await
    {
        return err_resp::<()>(StatusCode::UNAUTHORIZED, err).into_response();
    }

    // ── Direct-play progress tracking (off the critical path) ──
    if let Some(ref user_id) = query.dp_user {
        let byte_offset = parse_range_start(request.headers().get(header::RANGE));
        state.direct_play_tracker.update(
            user_id,
            &file_id,
            query.dp_movie.as_deref(),
            query.dp_episode.as_deref(),
            query.dp_dur.unwrap_or(0.0),
            query.dp_size.unwrap_or(0),
            byte_offset,
        );
    }

    let target = match MediaFileRepo::load_stream_target(&db, &file_id).await {
        Ok(Some(t)) => t,
        Ok(None) => return err404::<()>("Media file not found".into()).into_response(),
        Err(err) => return err500::<()>(format!("media file lookup failed: {err}")).into_response(),
    };

    if target.media_server_id.is_some() {
        return err_resp::<()>(
            StatusCode::BAD_REQUEST,
            "Media server-backed file must be streamed via its media server".into(),
        )
        .into_response();
    }

    if target.source_type.as_deref() == Some("local") {
        let response = match ServeFile::new(&target.path)
            .with_buf_chunk_size(LOCAL_MEDIA_STREAM_CHUNK_SIZE)
            .oneshot(request)
            .await
        {
            Ok(response) => response,
            Err(never) => match never {},
        };
        return response.map(Body::new).into_response();
    }

    let Some(source_type) = target.source_type.as_deref() else {
        return err404::<()>("Filesystem-backed media file not found".into()).into_response();
    };
    if !REMOTE_FS_SOURCE_TYPES.contains(&source_type) {
        return err_resp::<()>(
            StatusCode::BAD_REQUEST,
            format!("Unsupported filesystem source type: {source_type}"),
        )
        .into_response();
    }

    let Some(source_id) = target.source_id.as_deref() else {
        return err500::<()>("Filesystem source is missing source_id".into()).into_response();
    };

    let vfs = match state.sources.ensure_vfs(source_id).await {
        Ok(vfs) => vfs,
        Err(err) => return err404::<()>(err).into_response(),
    };

    let tap_tx = if query.probe_only.unwrap_or(false) {
        None
    } else {
        match SubtitleRepo::load_file_subtitles(&db, &file_id).await {
            Ok(rows) if !rows.is_empty() => {
                let ffprobe_raw = rows[0].ffprobe_raw.clone();
                let start_time_ms = extract_start_time_ms(&ffprobe_raw);
                let subs: Vec<_> = rows.iter().map(|r| r.to_embedded_record()).collect();
                let tracks = resolve_subtitle_tracks(&ffprobe_raw, &subs);
                build_stream_tap(
                    &state.subtitle_cache,
                    &state.tap_registry,
                    tracks,
                    &target.path,
                    start_time_ms,
                )
            }
            _ => None,
        }
    };

    stream_driver_file(vfs, target.path, request.headers().clone(), tap_tx).await
}

// ── Auth helpers ────────────────────────────────────────────────────────────

fn session_id_from_cookie(cookie_header: Option<&axum::http::HeaderValue>) -> Option<String> {
    let cookie_header = cookie_header?.to_str().ok()?;
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|cookie| cookie.strip_prefix("SESSION_ID=").map(ToOwned::to_owned))
}

async fn validate_stream_access(
    db: &PgPool,
    cookie_header: Option<&axum::http::HeaderValue>,
    access_token: Option<&str>,
    access_token_header: Option<&axum::http::HeaderValue>,
    client_ip: std::net::IpAddr,
) -> Result<(), String> {
    if client_ip.is_loopback() {
        return Ok(());
    }

    if let Some(token) = access_token {
        if AuthRepo::validate_internal_stream_token(db, token)
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }
    }
    if let Some(token) = access_token_header.and_then(|value| value.to_str().ok()) {
        if AuthRepo::validate_internal_stream_token(db, token)
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }
    }
    let session_id =
        session_id_from_cookie(cookie_header).ok_or_else(|| "Unauthorized".to_string())?;
    if AuthRepo::validate_session(db, &session_id)
        .await
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err("Unauthorized".into())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the start byte offset from an HTTP Range header (`bytes=START-...`).
fn parse_range_start(range: Option<&header::HeaderValue>) -> u64 {
    range
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("bytes="))
        .and_then(|s| s.split('-').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
