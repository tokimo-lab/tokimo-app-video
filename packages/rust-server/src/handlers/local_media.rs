use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde_json::Value;
use serde::Deserialize;
use std::{env, path::PathBuf, process::Stdio, sync::Arc};
use tokio::{io::AsyncWriteExt, process::Command, time::Duration};
use tokio_postgres::Client;
use tower::util::ServiceExt;
use tower_http::services::ServeFile;
use tracing::error;

use next_fs::Vfs;

use crate::{
    handlers::media_stream::stream_driver_file,
    handlers::{err404, err500, err_resp, ApiResponse},
    AppState,
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

struct EmbeddedSubtitleTarget {
    subtitle_id: String,
    file_id: String,
    path: String,
    source_id: Option<String>,
    source_type: Option<String>,
    media_server_id: Option<String>,
    format: String,
    ffprobe_raw: Option<Value>,
}

#[derive(Clone)]
struct EmbeddedSubtitleRecord {
    id: String,
    language: String,
    title: Option<String>,
    format: String,
    is_default: bool,
    is_forced: bool,
    source_id: Option<String>,
}

struct EmbeddedSubtitleOutput {
    output_format: &'static str,
    content_type: &'static str,
}

enum SubtitleSource {
    LocalPath(String),
    RemoteVfs { vfs: Arc<Vfs>, path: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamAccessQuery {
    access_token: Option<String>,
}

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
            format!("Unsupported filesystem source type: {}", source_type),
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

    stream_driver_file(vfs, target.path, request.headers().clone()).await
}

pub async fn stream_embedded_subtitle(
    State(state): State<Arc<AppState>>,
    Path(subtitle_id): Path<String>,
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

    let target = match load_embedded_subtitle_target(&db, &subtitle_id).await {
        Ok(target) => target,
        Err(response) => return response.into_response(),
    };

    if target.media_server_id.is_some() {
        return err_resp::<()>(
            StatusCode::BAD_REQUEST,
            "Media server-backed subtitle must be streamed via its media server".into(),
        )
        .into_response();
    }

    let Some(output) = get_embedded_subtitle_output(&target.format) else {
        return err_resp::<()>(
            StatusCode::BAD_REQUEST,
            format!("Unsupported subtitle format: {}", target.format),
        )
        .into_response();
    };

    let subtitles = match load_file_subtitles(&db, &target.file_id).await {
        Ok(subtitles) => subtitles,
        Err(response) => return response.into_response(),
    };

    let Some(stream_index) =
        resolve_embedded_subtitle_stream_index(&target.ffprobe_raw, &subtitles, &target.subtitle_id)
    else {
        return err_resp::<()>(StatusCode::NOT_FOUND, "Embedded subtitle stream not found".into())
            .into_response();
    };

    if let Some(cached) = read_subtitle_cache(&target.subtitle_id, output.output_format).await {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, output.content_type)
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(Bytes::from(cached)))
            .expect("subtitle response");
    }

    let source = match resolve_subtitle_source(&state, &target).await {
        Ok(source) => source,
        Err(response) => return response.into_response(),
    };

    let extracted = match extract_subtitle(&source, stream_index, output.output_format).await {
        Ok(buffer) => buffer,
        Err(err) => {
            error!(
                "embedded subtitle extraction failed for subtitle {}: {}",
                target.subtitle_id, err
            );
            return err_resp::<()>(StatusCode::INTERNAL_SERVER_ERROR, "Failed to extract subtitle".into())
                .into_response();
        }
    };

    write_subtitle_cache(&target.subtitle_id, output.output_format, &extracted).await;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, output.content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(Bytes::from(extracted)))
        .expect("subtitle response")
}

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

    let session_id = session_id_from_cookie(cookie_header).ok_or_else(|| "Unauthorized".to_string())?;
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

    if row.is_some() {
        Ok(())
    } else {
        Err("Unauthorized".into())
    }
}

async fn validate_internal_stream_token(db: &Client, access_token: &str) -> Result<(), String> {
    let row = db
        .query_opt(
            "SELECT 1 FROM system_settings WHERE internal_stream_access_token = $1 AND internal_stream_access_token_expires_at > NOW() LIMIT 1",
            &[&access_token],
        )
        .await
        .map_err(|err| {
            error!("internal stream token lookup failed: {}", err);
            "Internal token validation failed".to_string()
        })?;

    if row.is_some() {
        Ok(())
    } else {
        Err("Unauthorized".into())
    }
}

async fn load_media_file_stream_target(
    db: &Client,
    file_id: &str,
) -> Result<MediaFileStreamTarget, (StatusCode, axum::Json<ApiResponse<()>>)> {
    let row = db
        .query_opt(
            r#"
            SELECT
              mf.path,
              mf.source_id::text AS source_id,
              ms.type AS source_type,
              mf.media_server_id::text AS media_server_id
            FROM media_files mf
            LEFT JOIN media_sources ms ON ms.id = mf.source_id
            WHERE mf.id::text = $1
            "#,
            &[&file_id],
        )
        .await
        .map_err(|err| err500::<()>(format!("media file lookup failed: {}", err)))?;

    let Some(row) = row else {
        return Err(err404::<()>("Media file not found".into()));
    };

    let path: String = row
        .try_get("path")
        .map_err(|err| err500::<()>(format!("invalid media file path: {}", err)))?;
    let source_id = row
        .try_get("source_id")
        .map_err(|err| err500::<()>(format!("invalid source_id: {}", err)))?;
    let source_type = row
        .try_get("source_type")
        .map_err(|err| err500::<()>(format!("invalid source type: {}", err)))?;
    let media_server_id = row
        .try_get("media_server_id")
        .map_err(|err| err500::<()>(format!("invalid media_server_id: {}", err)))?;

    Ok(MediaFileStreamTarget {
        path,
        source_id,
        source_type,
        media_server_id,
    })
}

async fn load_embedded_subtitle_target(
    db: &Client,
    subtitle_id: &str,
) -> Result<EmbeddedSubtitleTarget, (StatusCode, axum::Json<ApiResponse<()>>)> {
    let row = db
        .query_opt(
            r#"
            SELECT
              s.id::text AS subtitle_id,
              s.file_id::text AS file_id,
              s.source_id AS subtitle_source_id,
              s.format,
              mf.path,
              mf.ffprobe_raw,
              mf.source_id::text AS source_id,
              ms.type AS source_type,
              mf.media_server_id::text AS media_server_id
            FROM subtitles s
            JOIN media_files mf ON mf.id = s.file_id
            LEFT JOIN media_sources ms ON ms.id = mf.source_id
            WHERE s.id::text = $1 AND s.source_type = 'embedded'
            "#,
            &[&subtitle_id],
        )
        .await
        .map_err(|err| err500::<()>(format!("subtitle lookup failed: {}", err)))?;

    let Some(row) = row else {
        return Err(err404::<()>("Embedded subtitle not found".into()));
    };

    Ok(EmbeddedSubtitleTarget {
        subtitle_id: row
            .try_get("subtitle_id")
            .map_err(|err| err500::<()>(format!("invalid subtitle_id: {}", err)))?,
        file_id: row
            .try_get("file_id")
            .map_err(|err| err500::<()>(format!("invalid file_id: {}", err)))?,
        path: row
            .try_get("path")
            .map_err(|err| err500::<()>(format!("invalid media file path: {}", err)))?,
        source_id: row
            .try_get("source_id")
            .map_err(|err| err500::<()>(format!("invalid source_id: {}", err)))?,
        source_type: row
            .try_get("source_type")
            .map_err(|err| err500::<()>(format!("invalid source_type: {}", err)))?,
        media_server_id: row
            .try_get("media_server_id")
            .map_err(|err| err500::<()>(format!("invalid media_server_id: {}", err)))?,
        format: row
            .try_get("format")
            .map_err(|err| err500::<()>(format!("invalid format: {}", err)))?,
        ffprobe_raw: row
            .try_get("ffprobe_raw")
            .map_err(|err| err500::<()>(format!("invalid ffprobe_raw: {}", err)))?,
    })
}

async fn load_file_subtitles(
    db: &Client,
    file_id: &str,
) -> Result<Vec<EmbeddedSubtitleRecord>, (StatusCode, axum::Json<ApiResponse<()>>)> {
    let rows = db
        .query(
            r#"
            SELECT
              id::text AS id,
              language,
              title,
              format,
              is_default,
              is_forced,
              source_id
            FROM subtitles
            WHERE file_id::text = $1 AND source_type = 'embedded'
            ORDER BY is_default DESC, language ASC, created_at ASC
            "#,
            &[&file_id],
        )
        .await
        .map_err(|err| err500::<()>(format!("subtitle list lookup failed: {}", err)))?;

    rows.into_iter()
        .map(|row| {
            Ok(EmbeddedSubtitleRecord {
                id: row
                    .try_get("id")
                    .map_err(|err| err500::<()>(format!("invalid subtitle id: {}", err)))?,
                language: row
                    .try_get("language")
                    .map_err(|err| err500::<()>(format!("invalid subtitle language: {}", err)))?,
                title: row
                    .try_get("title")
                    .map_err(|err| err500::<()>(format!("invalid subtitle title: {}", err)))?,
                format: row
                    .try_get("format")
                    .map_err(|err| err500::<()>(format!("invalid subtitle format: {}", err)))?,
                is_default: row
                    .try_get("is_default")
                    .map_err(|err| err500::<()>(format!("invalid subtitle is_default: {}", err)))?,
                is_forced: row
                    .try_get("is_forced")
                    .map_err(|err| err500::<()>(format!("invalid subtitle is_forced: {}", err)))?,
                source_id: row
                    .try_get("source_id")
                    .map_err(|err| err500::<()>(format!("invalid subtitle source_id: {}", err)))?,
            })
        })
        .collect()
}

fn get_embedded_subtitle_output(format: &str) -> Option<EmbeddedSubtitleOutput> {
    match format.trim().to_lowercase().as_str() {
        "subrip" | "srt" | "mov_text" => Some(EmbeddedSubtitleOutput {
            output_format: "srt",
            content_type: "text/plain; charset=utf-8",
        }),
        "webvtt" | "vtt" => Some(EmbeddedSubtitleOutput {
            output_format: "webvtt",
            content_type: "text/vtt; charset=utf-8",
        }),
        "ass" => Some(EmbeddedSubtitleOutput {
            output_format: "ass",
            content_type: "text/plain; charset=utf-8",
        }),
        "ssa" => Some(EmbeddedSubtitleOutput {
            output_format: "ssa",
            content_type: "text/plain; charset=utf-8",
        }),
        "hdmv_pgs_subtitle" | "pgs" | "sup" => Some(EmbeddedSubtitleOutput {
            output_format: "sup",
            content_type: "application/octet-stream",
        }),
        _ => None,
    }
}

fn parse_embedded_subtitle_source_index(source_id: Option<&str>) -> Option<i32> {
    source_id?.parse::<i32>().ok()
}

fn normalize_subtitle_signature_part(value: Option<&str>) -> String {
    value.unwrap_or_default().trim().to_lowercase()
}

fn build_subtitle_signature(
    language: Option<&str>,
    title: Option<&str>,
    format: Option<&str>,
    is_default: bool,
    is_forced: bool,
) -> String {
    [
        normalize_subtitle_signature_part(language),
        normalize_subtitle_signature_part(title),
        normalize_subtitle_signature_part(format),
        if is_default { "1".into() } else { "0".into() },
        if is_forced { "1".into() } else { "0".into() },
    ]
    .join("|")
}

fn build_relaxed_subtitle_signature(language: Option<&str>, format: Option<&str>) -> String {
    [
        normalize_subtitle_signature_part(language),
        normalize_subtitle_signature_part(format),
    ]
    .join("|")
}

fn resolve_embedded_subtitle_stream_index(
    raw_output: &Option<Value>,
    subtitles: &[EmbeddedSubtitleRecord],
    subtitle_id: &str,
) -> Option<i32> {
    let subtitle = subtitles.iter().find(|item| item.id == subtitle_id)?;
    if let Some(index) = parse_embedded_subtitle_source_index(subtitle.source_id.as_deref()) {
        return Some(index);
    }

    let streams = raw_output.as_ref()?.get("streams")?.as_array()?;
    let mut exact_buckets = std::collections::HashMap::<String, Vec<i32>>::new();
    let mut relaxed_buckets = std::collections::HashMap::<String, Vec<i32>>::new();

    for stream in streams {
        if stream.get("codec_type")?.as_str()? != "subtitle" {
            continue;
        }

        let index = stream.get("index")?.as_i64()?;
        let Ok(index) = i32::try_from(index) else {
            continue;
        };

        let tags = stream.get("tags");
        let disposition = stream.get("disposition");
        let language = tags
            .and_then(|value| value.get("language"))
            .and_then(Value::as_str)
            .unwrap_or("und");
        let title = tags
            .and_then(|value| value.get("title"))
            .and_then(Value::as_str)
            .or_else(|| {
                tags.and_then(|value| value.get("handler_name"))
                    .and_then(Value::as_str)
            });
        let format = stream.get("codec_name").and_then(Value::as_str);
        let is_default = disposition
            .and_then(|value| value.get("default"))
            .and_then(Value::as_i64)
            == Some(1);
        let is_forced = disposition
            .and_then(|value| value.get("forced"))
            .and_then(Value::as_i64)
            == Some(1);

        let exact_key = build_subtitle_signature(
            Some(language),
            title,
            format,
            is_default,
            is_forced,
        );
        let relaxed_key = build_relaxed_subtitle_signature(Some(language), format);
        exact_buckets.entry(exact_key).or_default().push(index);
        relaxed_buckets.entry(relaxed_key).or_default().push(index);
    }

    for item in subtitles {
        let exact_key = build_subtitle_signature(
            Some(item.language.as_str()),
            item.title.as_deref(),
            Some(item.format.as_str()),
            item.is_default,
            item.is_forced,
        );
        let relaxed_key =
            build_relaxed_subtitle_signature(Some(item.language.as_str()), Some(item.format.as_str()));

        if let Some(queue) = exact_buckets.get_mut(&exact_key) {
            if !queue.is_empty() {
                let next = queue.remove(0);
                if item.id == subtitle_id {
                    return Some(next);
                }
                if let Some(relaxed_queue) = relaxed_buckets.get_mut(&relaxed_key) {
                    if let Some(position) = relaxed_queue.iter().position(|value| *value == next) {
                        relaxed_queue.remove(position);
                    }
                }
                continue;
            }
        }

        if let Some(queue) = relaxed_buckets.get_mut(&relaxed_key) {
            if !queue.is_empty() {
                let next = queue.remove(0);
                if item.id == subtitle_id {
                    return Some(next);
                }
            }
        }
    }

    None
}

async fn resolve_subtitle_source(
    state: &AppState,
    target: &EmbeddedSubtitleTarget,
) -> Result<SubtitleSource, (StatusCode, axum::Json<ApiResponse<()>>)> {
    if target.source_type.as_deref() == Some("local") {
        return Ok(SubtitleSource::LocalPath(target.path.clone()));
    }

    let Some(source_type) = target.source_type.as_deref() else {
        return Err(err404::<()>(
            "Filesystem-backed media file not found".into(),
        ));
    };

    if !REMOTE_FS_SOURCE_TYPES.contains(&source_type) {
        return Err(err_resp::<()>(
            StatusCode::BAD_REQUEST,
            format!("Unsupported filesystem source type: {}", source_type),
        ));
    }

    let source_id = target
        .source_id
        .as_deref()
        .ok_or_else(|| err500::<()>("Filesystem source is missing source_id".into()))?;

    let vfs = state
        .sources
        .ensure_vfs(source_id)
        .await
        .map_err(|err| err404::<()>(err))?;

    Ok(SubtitleSource::RemoteVfs {
        vfs,
        path: target.path.clone(),
    })
}

fn subtitle_cache_path(subtitle_id: &str, output_format: &str) -> PathBuf {
    let cache_dir = env::var("SUBTITLE_CACHE_DIR")
        .unwrap_or_else(|_| "/tmp/my-media-subtitles".to_string());
    PathBuf::from(cache_dir).join(format!("{}.{}", subtitle_id, output_format))
}

async fn read_subtitle_cache(subtitle_id: &str, output_format: &str) -> Option<Vec<u8>> {
    let path = subtitle_cache_path(subtitle_id, output_format);
    tokio::fs::read(&path).await.ok()
}

async fn write_subtitle_cache(subtitle_id: &str, output_format: &str, data: &[u8]) {
    let path = subtitle_cache_path(subtitle_id, output_format);
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let _ = tokio::fs::write(&path, data).await;
}

async fn extract_subtitle(
    source: &SubtitleSource,
    stream_index: i32,
    output_format: &str,
) -> Result<Vec<u8>, String> {
    match source {
        SubtitleSource::LocalPath(path) => {
            extract_subtitle_from_path(path, stream_index, output_format).await
        }
        SubtitleSource::RemoteVfs { vfs, path } => {
            extract_subtitle_from_vfs(Arc::clone(vfs), path.clone(), stream_index, output_format)
                .await
        }
    }
}

async fn extract_subtitle_from_path(
    path: &str,
    stream_index: i32,
    output_format: &str,
) -> Result<Vec<u8>, String> {
    let output = Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(path)
        .arg("-copyts")
        .arg("-an")
        .arg("-vn")
        .arg("-map")
        .arg(format!("0:{}", stream_index))
        .arg("-f")
        .arg(output_format)
        .arg("-")
        .output()
        .await
        .map_err(|err| format!("failed to spawn ffmpeg: {}", err))?;

    if output.status.success() {
        return Ok(output.stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "ffmpeg exited with status {:?}: {}",
        output.status.code(),
        stderr.trim()
    ))
}

async fn extract_subtitle_from_vfs(
    vfs: Arc<Vfs>,
    path: String,
    stream_index: i32,
    output_format: &str,
) -> Result<Vec<u8>, String> {
    let timeout_secs = env::var("SUBTITLE_EXTRACT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300);

    let mut child = Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg("pipe:0")
        .arg("-copyts")
        .arg("-an")
        .arg("-vn")
        .arg("-map")
        .arg(format!("0:{}", stream_index))
        .arg("-f")
        .arg(output_format)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| format!("failed to spawn ffmpeg: {}", err))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open ffmpeg stdin".to_string())?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
    let path_buf = PathBuf::from(&path);

    tokio::spawn(async move {
        vfs.stream_to(&path_buf, 0, None, tx).await;
    });

    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            if stdin.write_all(&chunk).await.is_err() {
                break;
            }
        }
    });

    match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(output.stdout)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!(
                    "ffmpeg exited with status {:?}: {}",
                    output.status.code(),
                    stderr.trim()
                ))
            }
        }
        Ok(Err(err)) => Err(format!("ffmpeg process error: {}", err)),
        Err(_) => Err(format!(
            "subtitle extraction timed out after {}s",
            timeout_secs
        )),
    }
}
