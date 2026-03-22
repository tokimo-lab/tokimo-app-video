use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sea_orm::DatabaseConnection;
use std::{net::SocketAddr, sync::Arc};
use tower::util::ServiceExt;
use tower_http::services::ServeFile;

use crate::{
    db::repos::{
        auth_repo::AuthRepo, media::file_repo::MediaFileRepo, subtitle_repo::SubtitleRepo,
    },
    handlers::media::stream::stream_driver_file,
    handlers::{err404, err500, err_resp},
    AppState,
};

use rust_subtitle::{
    resolve::{extract_start_time_ms, resolve_subtitle_tracks},
    tap_builder::build_stream_tap,
};

const LOCAL_MEDIA_STREAM_CHUNK_SIZE: usize = 1024 * 1024;
const REMOTE_FS_SOURCE_TYPES: [&str; 10] = [
    "smb", "nfs", "webdav", "ftp", "sftp", "s3",
    "115cloud", "aliyundrive", "baidu_netdisk", "quark",
];
const INTERNAL_STREAM_ACCESS_HEADER: &str = "x-internal-stream-access-token";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamAccessQuery {
    access_token: Option<String>,
    probe_only: Option<bool>,
    dp_user: Option<String>,
    dp_movie: Option<String>,
    dp_episode: Option<String>,
    dp_dur: Option<f64>,
    dp_size: Option<u64>,
}

pub async fn stream_media_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    Query(query): Query<StreamAccessQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
) -> Response {
    let db = state.sources.db_conn();
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
        Ok(Some(target)) => target,
        Ok(None) => return err404::<()>("Media file not found".into()).into_response(),
        Err(err) => {
            return err500::<()>(format!("media file lookup failed: {err}")).into_response();
        }
    };

    if target.media_server_id.is_some() {
        return err_resp::<()>(
            StatusCode::BAD_REQUEST,
            "Media server-backed file must be streamed via its media server".into(),
        )
        .into_response();
    }

    // Build subtitle tap for embedded subtitles (needed for SSE streaming).
    let tap_tx = if query.probe_only.unwrap_or(false) {
        None
    } else {
        match SubtitleRepo::load_file_subtitles(&db, &file_id).await {
            Ok(rows) if !rows.is_empty() => {
                let ffprobe_raw = rows[0].ffprobe_raw.clone();
                let start_time_ms = extract_start_time_ms(&ffprobe_raw);
                let subs: Vec<_> = rows.iter().map(|row| row.to_embedded_record()).collect();
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

    if target.source_type.as_deref() == Some("local") {
        // When embedded subtitles need tapping, stream through VFS so chunks
        // are fed to the subtitle extractor. Otherwise use ServeFile for efficiency.
        if tap_tx.is_some() {
            if let Some(source_id) = target.source_id.as_deref() {
                if let Ok(vfs) = state.sources.ensure_vfs(source_id).await {
                    return stream_driver_file(vfs, target.path, request.headers().clone(), tap_tx)
                        .await;
                }
            }
        }

        let abs_path = resolve_local_path(&target.path, target.source_config.as_ref());
        let response = match ServeFile::new(&abs_path)
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

    stream_driver_file(vfs, target.path, request.headers().clone(), tap_tx).await
}

fn session_id_from_cookie(cookie_header: Option<&axum::http::HeaderValue>) -> Option<String> {
    let cookie_header = cookie_header?.to_str().ok()?;
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|cookie| cookie.strip_prefix("SESSION_ID=").map(ToOwned::to_owned))
}

async fn validate_stream_access(
    db: &DatabaseConnection,
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

fn parse_range_start(range: Option<&header::HeaderValue>) -> u64 {
    range
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("bytes="))
        .and_then(|value| value.split('-').next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

/// Resolve the absolute local filesystem path by prepending the source's
/// `root_folder_path` (or `root` / `path`) from its config JSON.
pub(crate) fn resolve_local_path(rel_path: &str, config: Option<&serde_json::Value>) -> String {
    let driver_root = config
        .and_then(|c| {
            c.get("root")
                .or_else(|| c.get("root_folder_path"))
                .or_else(|| c.get("path"))
        })
        .and_then(|v| v.as_str());
    match driver_root {
        Some(root) => format!("{}{}", root.trim_end_matches('/'), rel_path),
        None => rel_path.to_string(),
    }
}