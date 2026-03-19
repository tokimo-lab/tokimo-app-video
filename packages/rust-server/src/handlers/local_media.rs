use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
};
use bytes::Bytes;
use serde_json::Value;
use serde::Deserialize;
use std::{env, path::PathBuf, process::Stdio, sync::Arc};
use tokio::{io::AsyncWriteExt, process::Command, time::Duration};
use tokio_postgres::Client;
use tower::util::ServiceExt;
use tower_http::services::ServeFile;
use tracing::{error, info, warn};

use next_fs::Vfs;

use crate::{
    handlers::media_stream::stream_driver_file,
    handlers::{err404, err500, err_resp, ApiResponse},
    mkv_tap::SubtitleEvent,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleEventsQuery {
    start_ms: Option<f64>,
    end_ms: Option<f64>,
    #[allow(dead_code)]
    access_token: Option<String>,
}

/// GET /api/subtitles/{subtitle_id}/events?startMs=&endMs=
///
/// Returns cached subtitle events for a time window.  The events are populated
/// by the MKV stream tap that runs in the background when the player starts
/// streaming the file.  Falls back to an empty list if extraction is not yet
/// complete for the requested window.
pub async fn get_subtitle_events(
    State(state): State<Arc<AppState>>,
    Path(subtitle_id): Path<String>,
    Query(query): Query<SubtitleEventsQuery>,
) -> Response {
    let start_ms = query.start_ms.unwrap_or(0.0) as i64;
    let end_ms = query.end_ms.unwrap_or(i64::MAX as f64) as i64;

    match state.subtitle_cache.query(&subtitle_id, start_ms, end_ms) {
        Some((events, complete)) => {
            let body = serde_json::json!({
                "events": events,
                "complete": complete,
            });
            axum::Json(body).into_response()
        }
        None => {
            // Not in cache yet — return empty with complete=false so the
            // frontend can retry after a short delay.
            let body = serde_json::json!({
                "events": [],
                "complete": false,
            });
            axum::Json(body).into_response()
        }
    }
}

// ── SSE subtitle stream ──────────────────────────────────────────────────────

/// SSE endpoint: streams subtitle events as they are extracted from the player
/// byte stream.  First sends all cached events, then pushes new ones in real
/// time via broadcast channel.  The connection stays open until the client
/// disconnects.
pub async fn subtitle_events_sse(
    State(state): State<Arc<AppState>>,
    Path(subtitle_id): Path<String>,
    Query(_query): Query<SubtitleEventsQuery>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let (snapshot, mut rx) = state.subtitle_cache.subscribe(&subtitle_id);

    info!(
        "[SSE] subscriber connected for sub={}, snapshot={} events",
        subtitle_id,
        snapshot.len()
    );

    let sub_id = subtitle_id.clone();
    let stream = async_stream::stream! {
        // 1. Send all cached events
        for ev in &snapshot {
            let json = serde_json::to_string(ev).unwrap_or_default();
            yield Ok::<_, std::convert::Infallible>(Event::default().data(json));
        }

        // 2. Stream new events as they arrive from the tap
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

    // Build a tap channel for MKV subtitle extraction (if applicable).
    // The tee in stream_driver_file will forward every byte to the tap at zero
    // extra I/O cost — no second SMB read.
    let tap_tx = build_mkv_tap_channel(&state, &db, &file_id, &target.path).await;

    stream_driver_file(vfs, target.path, request.headers().clone(), tap_tx).await
}

/// Build the tap sender for MKV subtitle extraction.
///
/// Returns `Some(tap_tx)` when:
/// - the file is `.mkv` or `.webm`
/// - there are embedded text subtitles not yet fully cached
///
/// The tap is **persistent per file**: header parsed from the first Range
/// request is reused for all subsequent requests.  The tap task runs in the
/// background, receiving `(chunk, offset)` from the tee inside
/// `stream_driver_file` and feeding them into `MkvStreamTap`.
async fn build_mkv_tap_channel(
    state: &AppState,
    db: &tokio_postgres::Client,
    file_id: &str,
    path: &str,
) -> Option<tokio::sync::mpsc::Sender<(Vec<u8>, u64)>> {
    let path_lower = path.to_lowercase();
    if !path_lower.ends_with(".mkv") && !path_lower.ends_with(".webm") {
        return None;
    }

    let subs = match load_file_subtitles_with_ffprobe(db, file_id).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[MkvTap] could not load subtitles for {}: {:?}", file_id, e);
            return None;
        }
    };

    // Build track_map (track_num → subtitle_id) for tap, and
    // format_map (subtitle_id → format) for text cleaning.
    let mut format_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let track_map: Vec<(u64, String)> = subs
        .into_iter()
        .filter(|(_, sub_id, _)| !state.subtitle_cache.is_complete(sub_id))
        .filter(|(_, _, fmt)| is_text_subtitle_format(fmt))
        .filter_map(|(track_num, sub_id, fmt)| {
            format_map.insert(sub_id.clone(), fmt);
            Some((track_num?, sub_id))
        })
        .collect();

    if track_map.is_empty() {
        return None;
    }

    info!(
        "[MkvTap] tapping {} ({} subtitle track(s): {:?})",
        path,
        track_map.len(),
        track_map.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );

    // Get or create persistent tap for this file (survives across Range requests)
    let (tap, _subtitle_ids) = state.tap_registry.get_or_create(file_id, track_map);

    // Channel for this request — large capacity so fast streaming doesn't
    // drop chunks via try_send in the tee.
    let (tap_tx, mut tap_rx) = tokio::sync::mpsc::channel::<(Vec<u8>, u64)>(128);

    let cache = state.subtitle_cache.clone();

    tokio::spawn(async move {
        let mut chunk_count = 0usize;
        let mut total_events = 0usize;

        while let Some((chunk, offset)) = tap_rx.recv().await {
            chunk_count += 1;
            let events = {
                let mut t = tap.lock().unwrap();
                t.feed(&chunk, offset)
            };
            for (sub_id, evts) in events {
                let fmt = format_map.get(&sub_id).map(|s| s.as_str()).unwrap_or("");
                let cleaned: Vec<SubtitleEvent> = evts
                    .into_iter()
                    .map(|mut ev| {
                        ev.text = ev.text.map(|t| clean_subtitle_text(&t, fmt));
                        ev
                    })
                    .collect();
                if !cleaned.is_empty() {
                    info!(
                        "[MkvTap] +{} events for sub={} (offset={})",
                        cleaned.len(),
                        sub_id,
                        offset,
                    );
                }
                total_events += cleaned.len();
                cache.append(&sub_id, cleaned);
            }
        }

        info!(
            "[MkvTap] tap task finished: {} chunks, {} events",
            chunk_count, total_events
        );
    });

    Some(tap_tx)
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

fn is_text_subtitle_format(format: &str) -> bool {
    matches!(
        format.trim().to_lowercase().as_str(),
        "subrip" | "srt" | "mov_text" | "webvtt" | "vtt" | "ass" | "ssa"
    )
}

/// Clean raw MKV subtitle text based on format.
///
/// - ASS/SSA: strip the dialogue prefix (8 commas), remove override tags `{\...}`,
///   convert `\N` / `\n` / `\h` to real whitespace.
/// - SRT/VTT: strip HTML-like tags `<i>`, `<b>`, etc.
fn clean_subtitle_text(raw: &str, format: &str) -> String {
    let fmt = format.trim().to_lowercase();
    match fmt.as_str() {
        "ass" | "ssa" => strip_ass_text(raw),
        _ => strip_html_tags(raw),
    }
}

fn strip_ass_text(raw: &str) -> String {
    // ASS dialogue block: ReadOrder,Layer,Style,Actor,MarginL,MarginR,MarginV,Effect,Text
    // Skip 8 commas to get to the text field
    let mut commas = 0;
    for (i, ch) in raw.char_indices() {
        if ch == ',' {
            commas += 1;
            if commas == 8 {
                let text = &raw[i + 1..];
                // Remove override tags: {\pos(x,y)}, {\b1}, {\an8}, etc.
                let text = remove_ass_tags(text);
                // Convert line-break codes
                let text = text
                    .replace("\\N", "\n")
                    .replace("\\n", "\n")
                    .replace("\\h", " ");
                return text.trim().to_string();
            }
        }
    }
    // Not an ASS block prefix — return as-is with tags stripped
    let text = remove_ass_tags(raw);
    text.replace("\\N", "\n")
        .replace("\\n", "\n")
        .replace("\\h", " ")
        .trim()
        .to_string()
}

fn remove_ass_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '{' {
            in_tag = true;
        } else if ch == '}' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }
    result
}

fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }
    result
}

/// Load embedded subtitles for a file and resolve their MKV track numbers.
///
/// Returns `Vec<(Option<track_number>, subtitle_id, format)>`.
/// Track number is `None` if it cannot be resolved from the DB `source_id` or
/// ffprobe data.
async fn load_file_subtitles_with_ffprobe(
    db: &Client,
    file_id: &str,
) -> Result<Vec<(Option<u64>, String, String)>, String> {
    let rows = db
        .query(
            r#"
            SELECT
              s.id::text AS id,
              s.language,
              s.title,
              s.format,
              s.is_default,
              s.is_forced,
              s.source_id,
              mf.ffprobe_raw
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

    // Parse ffprobe once (all rows have same file's ffprobe_raw)
    let ffprobe_raw: Option<Value> = rows[0].try_get("ffprobe_raw").unwrap_or(None);

    // Build the same EmbeddedSubtitleRecord list so we can use existing helpers
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

    let result = subs
        .iter()
        .map(|sub| {
            let stream_index =
                resolve_embedded_subtitle_stream_index(&ffprobe_raw, &subs, &sub.id);
            // stream_index is the ffprobe stream index (0-based for the whole file)
            // We need the MKV track number.
            // The MKV track number for subtitle stream at ffprobe index N can be
            // found from the ffprobe streams array: stream.tags.NUMBER_OF_FRAMES etc.
            // are unreliable, but the `source_id` field stores the 0-based subtitle
            // stream index within the subtitle streams.
            //
            // Simplest mapping: use ffprobe stream index → look up in ffprobe streams
            // array for the `codec_name` == subtitle and find the track_number tag.
            // Fallback: use stream_index as track number + 1 (MKV tracks are 1-based).
            let track_num = resolve_mkv_track_number(&ffprobe_raw, stream_index);
            (track_num, sub.id.clone(), sub.format.clone())
        })
        .collect();

    Ok(result)
}

/// Resolve a MKV track number from ffprobe data.
///
/// ffprobe `streams[i].tags["NUMBER_OF_FRAMES"]` or similar tags are not
/// reliable.  The best approach is to use the stream index directly: MKV
/// tracks are 1-indexed and their order usually matches the ffprobe stream
/// order.  We add 1 to get the track number.
fn resolve_mkv_track_number(ffprobe_raw: &Option<Value>, stream_index: Option<i32>) -> Option<u64> {
    let idx = stream_index?;

    // Try to read from ffprobe streams the DURATION_TS or track_number tag
    if let Some(streams) = ffprobe_raw.as_ref()?.get("streams")?.as_array() {
        if let Some(stream) = streams.get(idx as usize) {
            // Some muxers store the MKV track number in tags
            if let Some(track_num) = stream
                .get("tags")
                .and_then(|t| t.get("TRACK_NUMBER").or_else(|| t.get("track_number")))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
            {
                return Some(track_num);
            }
        }
    }

    // Fallback: MKV track numbers are 1-based and contiguous, so stream_index+1
    // is a reasonable approximation when ffprobe doesn't expose the track number
    // explicitly.
    Some(idx as u64 + 1)
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
