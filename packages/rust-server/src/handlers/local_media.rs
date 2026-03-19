use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
};

use serde::Deserialize;
use std::sync::Arc;
use tokio_postgres::Client;
use tower::util::ServiceExt;
use tower_http::services::ServeFile;
use tracing::{error, info};

use crate::{
    handlers::media_stream::stream_driver_file,
    handlers::{err404, err500, err_resp, ApiResponse},
    AppState,
};

use rust_subtitle::{
    resolve::resolve_subtitle_tracks,
    tap_builder::build_stream_tap,
    types::EmbeddedSubtitleRecord,
};

const LOCAL_MEDIA_STREAM_CHUNK_SIZE: usize = 1024 * 1024;
const REMOTE_FS_SOURCE_TYPES: [&str; 6] = ["smb", "nfs", "webdav", "ftp", "sftp", "s3"];
const INTERNAL_STREAM_ACCESS_HEADER: &str = "x-internal-stream-access-token";

struct MediaFileStreamTarget {
    path: String,
    source_id: Option<String>,
    source_type: Option<String>,
    media_server_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamAccessQuery {
    access_token: Option<String>,
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
            yield Ok::<_, std::convert::Infallible>(Event::default().data(json));
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    info!("[SSE] pushing event to sub={}: timeMs={}", sub_id, ev.time_ms);
                    let json = serde_json::to_string(&ev).unwrap_or_default();
                    yield Ok(Event::default().data(json));
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
    request: Request,
) -> Response {
    let db = state.sources.db_client();
    if let Err(err) = validate_stream_access(
        &db,
        request.headers().get(header::COOKIE),
        query.access_token.as_deref(),
        request.headers().get(INTERNAL_STREAM_ACCESS_HEADER),
    )
    .await
    {
        return err_resp::<()>(StatusCode::UNAUTHORIZED, err).into_response();
    }

    let target = match load_media_file_stream_target(&db, &file_id).await {
        Ok(target) => target,
        Err(response) => return response.into_response(),
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

    let tap_tx = {
        let subs = load_file_subtitles_with_tracks(&db, &file_id).await.unwrap_or_default();
        build_stream_tap(
            &state.subtitle_cache,
            &state.tap_registry,
            subs,
            &target.path,
        )
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
    db: &Client,
    cookie_header: Option<&axum::http::HeaderValue>,
    access_token: Option<&str>,
    access_token_header: Option<&axum::http::HeaderValue>,
) -> Result<(), String> {
    if let Some(token) = access_token {
        if validate_internal_stream_token(db, token).await.is_ok() {
            return Ok(());
        }
    }
    if let Some(token) = access_token_header.and_then(|value| value.to_str().ok()) {
        if validate_internal_stream_token(db, token).await.is_ok() {
            return Ok(());
        }
    }
    let session_id =
        session_id_from_cookie(cookie_header).ok_or_else(|| "Unauthorized".to_string())?;
    validate_session(db, &session_id).await
}

async fn validate_session(db: &Client, session_id: &str) -> Result<(), String> {
    let row = db
        .query_opt(
            "SELECT 1 FROM sessions WHERE id::text = $1 AND expires_at > NOW()",
            &[&session_id],
        )
        .await
        .map_err(|err| {
            error!("local media session lookup failed: {}", err);
            "Session validation failed".to_string()
        })?;
    if row.is_some() { Ok(()) } else { Err("Unauthorized".into()) }
}

async fn validate_internal_stream_token(db: &Client, access_token: &str) -> Result<(), String> {
    let row = db
        .query_opt(
            "SELECT 1 FROM system_settings WHERE internal_stream_access_token = $1 \
             AND internal_stream_access_token_expires_at > NOW() LIMIT 1",
            &[&access_token],
        )
        .await
        .map_err(|err| {
            error!("internal stream token lookup failed: {}", err);
            "Internal token validation failed".to_string()
        })?;
    if row.is_some() { Ok(()) } else { Err("Unauthorized".into()) }
}

// ── DB / VFS helpers ────────────────────────────────────────────────────────

async fn load_media_file_stream_target(
    db: &Client,
    file_id: &str,
) -> Result<MediaFileStreamTarget, (StatusCode, axum::Json<ApiResponse<()>>)> {
    let row = db
        .query_opt(
            "SELECT mf.path, mf.source_id::text AS source_id, ms.type AS source_type, \
             mf.media_server_id::text AS media_server_id \
             FROM media_files mf LEFT JOIN media_sources ms ON ms.id = mf.source_id \
             WHERE mf.id::text = $1",
            &[&file_id],
        )
        .await
        .map_err(|err| err500::<()>(format!("media file lookup failed: {err}")))?;

    let Some(row) = row else {
        return Err(err404::<()>("Media file not found".into()));
    };

    Ok(MediaFileStreamTarget {
        path: row.try_get("path").map_err(|e| err500::<()>(format!("invalid path: {e}")))?,
        source_id: row.try_get("source_id").map_err(|e| err500::<()>(format!("invalid source_id: {e}")))?,
        source_type: row.try_get("source_type").map_err(|e| err500::<()>(format!("invalid source_type: {e}")))?,
        media_server_id: row.try_get("media_server_id").map_err(|e| err500::<()>(format!("invalid media_server_id: {e}")))?,
    })
}

async fn load_file_subtitles_with_tracks(
    db: &Client,
    file_id: &str,
) -> Result<Vec<(Option<u64>, String, String)>, String> {
    let rows = db
        .query(
            r#"
            SELECT
              s.id::text AS id, s.language, s.title, s.format,
              s.is_default, s.is_forced, s.source_id, mf.ffprobe_raw
            FROM subtitles s
            JOIN media_files mf ON mf.id = s.file_id
            WHERE s.file_id::text = $1 AND s.source_type = 'embedded'
            ORDER BY s.is_default DESC, s.language ASC, s.created_at ASC
            "#,
            &[&file_id],
        )
        .await
        .map_err(|err| format!("subtitle+ffprobe query failed: {}", err))?;

    if rows.is_empty() {
        return Ok(vec![]);
    }

    let ffprobe_raw: Option<serde_json::Value> = rows[0].try_get("ffprobe_raw").unwrap_or(None);

    let subs: Vec<EmbeddedSubtitleRecord> = rows
        .iter()
        .filter_map(|row| {
            Some(EmbeddedSubtitleRecord {
                id: row.try_get("id").ok()?,
                language: row.try_get("language").ok()?,
                title: row.try_get("title").ok().flatten(),
                format: row.try_get("format").ok()?,
                is_default: row.try_get("is_default").ok()?,
                is_forced: row.try_get("is_forced").ok()?,
                source_id: row.try_get("source_id").ok().flatten(),
            })
        })
        .collect();

    Ok(resolve_subtitle_tracks(&ffprobe_raw, &subs))
}
