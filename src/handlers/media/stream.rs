use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use std::fmt::Write as _;
use std::{convert::Infallible, path::Path, path::PathBuf, sync::Arc};
use tokimo_vfs::Vfs;
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use crate::handlers::err500;

const STREAM_CHANNEL_CAPACITY: usize = 8;
const STREAM_RANGE_BUFFER_LIMIT: u64 = 256 * 1024 * 1024;
const STREAM_RANGE_CHUNK_BYTES: u64 = 1024 * 1024;
const STREAM_RANGE_CHANNEL_MAX: usize = 32;

/// Stream a VFS file to an HTTP response.
///
/// If `tap` is provided, every chunk of bytes is **also** sent to the tap
/// channel as `(chunk, file_offset)` — no second VFS read is needed.
///
/// If `cancel` is provided (i.e. not already cancelled), the VFS task and
/// tee task will abort immediately when the token is cancelled. This is the
/// session lifecycle integration point.
pub async fn stream_driver_file(
    vfs: Arc<Vfs>,
    path: String,
    headers: HeaderMap,
    tap: Option<mpsc::Sender<(Bytes, u64)>>,
    cancel: CancellationToken,
) -> Response {
    let file_info = match vfs.stat(Path::new(&path)).await {
        Ok(file_info) => file_info,
        Err(err) => {
            error!("stream stat failed: {}", err);
            return err500::<()>(err.to_string()).into_response();
        }
    };
    let total = file_info.size;
    let range = parse_range(headers.get(header::RANGE), total);

    let channel_capacity = if range.status == StatusCode::PARTIAL_CONTENT && range.length <= STREAM_RANGE_BUFFER_LIMIT {
        range
            .length
            .div_ceil(STREAM_RANGE_CHUNK_BYTES)
            .clamp(STREAM_CHANNEL_CAPACITY as u64, STREAM_RANGE_CHANNEL_MAX as u64) as usize
    } else {
        STREAM_CHANNEL_CAPACITY
    };

    let (vfs_tx, vfs_rx) = mpsc::channel::<Vec<u8>>(channel_capacity);

    let path_buf = PathBuf::from(&path);
    let range_offset = range.offset;
    let range_length_opt = if range.open_ended { None } else { Some(range.length) };

    // VFS → vfs_tx
    let cancel_vfs = cancel.clone();
    tokio::spawn(async move {
        debug!("stream: vfs-task started path={path_buf:?} offset={range_offset} len={range_length_opt:?}");
        tokio::select! {
            () = cancel_vfs.cancelled() => {
                debug!("stream: vfs-task cancelled path={path_buf:?}");
            }
            () = vfs.stream_to(&path_buf, range_offset, range_length_opt, vfs_tx) => {
                debug!("stream: vfs-task ended path={path_buf:?}");
            }
        }
    });

    let need_tee = tap.is_some();

    let body = if need_tee {
        // Need tee: vfs_rx → player_tx (always) + tap (if provided)
        let (player_tx, player_rx) = mpsc::channel::<Bytes>(channel_capacity);
        let mut vfs_rx = vfs_rx;
        tokio::spawn(async move {
            debug!("stream: tee-task started");
            // When this guard is dropped (task exits for any reason), it cancels the
            // token — signalling StreamSessionManager that the stream is finished.
            // cleanup_stale sees is_cancelled()==true and removes the entry.
            let _done_guard = cancel.clone().drop_guard();
            let mut offset = range_offset;
            loop {
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => {
                        debug!("stream: tee-task cancelled at offset={}", offset);
                        return;
                    }
                    chunk_opt = vfs_rx.recv() => {
                        let Some(chunk) = chunk_opt else {
                            debug!("stream: tee-task ended offset={}", offset);
                            return;
                        };
                        let len = chunk.len() as u64;
                        let chunk = Bytes::from(chunk);
                        if let Some(ref tap_tx) = tap {
                            let _ = tap_tx.try_send((chunk.clone(), offset));
                        }
                        // Block until the player channel has capacity or the session is cancelled.
                        // No timeout / stall counting — session CancellationToken is the single
                        // exit mechanism (stop-session, browser crash → cleanup_stale after 60s).
                        tokio::select! {
                            biased;
                            () = cancel.cancelled() => {
                                debug!("stream: tee-task cancelled at offset={}", offset);
                                return;
                            }
                            result = player_tx.reserve() => {
                                if let Ok(permit) = result { permit.send(chunk) } else {
                                    // player_rx dropped (browser closed connection)
                                    debug!("stream: tee-task player_rx closed, stopping");
                                    return;
                                }
                            }
                        }
                        offset += len;
                    }
                }
            }
        });

        Body::from_stream(ReceiverStream::new(player_rx).map(Ok::<Bytes, Infallible>))
    } else {
        // No tap / no stats → zero-copy: pipe VFS channel directly to body
        Body::from_stream(ReceiverStream::new(vfs_rx).map(|chunk| Ok::<Bytes, Infallible>(Bytes::from(chunk))))
    };

    // Extract filename for Content-Disposition
    let filename = Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");
    let encoded_filename = percent_encode_filename(filename);

    let mut builder = Response::builder()
        .status(range.status)
        .header(header::CONTENT_TYPE, mime_for(&path))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_LENGTH, range.length.to_string())
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "inline; filename=\"{}\"; filename*=UTF-8''{}",
                filename.replace('"', "\\\""),
                encoded_filename
            ),
        );

    if range.status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!(
                "bytes {}-{}/{}",
                range.offset,
                range.offset + range.length.saturating_sub(1),
                total
            ),
        );
    }

    builder
        .body(body)
        .unwrap_or_else(|_| err500::<()>("failed to build stream response".into()).into_response())
}

/// Percent-encode a filename for use in `filename*=UTF-8''...` (RFC 5987).
fn percent_encode_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len() * 3);
    for byte in name.bytes() {
        match byte {
            // unreserved chars per RFC 5987 attr-char
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                write!(out, "{byte:02X}").ok();
            }
        }
    }
    out
}

struct ParsedRange {
    offset: u64,
    length: u64,
    status: StatusCode,
    open_ended: bool,
}

fn parse_range(range_header: Option<&axum::http::HeaderValue>, total: u64) -> ParsedRange {
    if let Some(val) = range_header
        && let Ok(s) = val.to_str()
        && let Some(rest) = s.strip_prefix("bytes=")
    {
        let parts: Vec<&str> = rest.splitn(2, '-').collect();
        if parts.len() == 2 {
            let start = parts[0].parse::<u64>().unwrap_or(0);
            let has_end = !parts[1].is_empty();
            let end = if has_end {
                parts[1].parse::<u64>().unwrap_or(total.saturating_sub(1))
            } else {
                total.saturating_sub(1)
            };
            let end = end.min(total.saturating_sub(1));
            if start <= end {
                return ParsedRange {
                    offset: start,
                    length: end - start + 1,
                    status: StatusCode::PARTIAL_CONTENT,
                    open_ended: !has_end,
                };
            }
        }
    }
    ParsedRange {
        offset: 0,
        length: total,
        status: StatusCode::OK,
        open_ended: false,
    }
}

pub(crate) fn mime_for(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "webm" => "video/webm",
        "ts" | "mpeg" | "mpg" => "video/mpeg",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "nfo" | "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
