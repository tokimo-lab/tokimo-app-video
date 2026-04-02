use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rust_hls::types::{AudioStreamInfo as HlsAudioStream, CreateSessionRequest, TonemapOptions};
use rust_subtitle::{
    resolve::{extract_start_time_ms, resolve_subtitle_tracks},
    tap_builder::build_stream_tap,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::db::entities::{file_systems, media_files};
use crate::db::models::playback::{
    AudioStreamInfo, ResumePositionDto, StreamUrlDto, WatchHistoryItemDto,
};
use crate::db::repos::media::PlaybackRepo;
use crate::db::repos::subtitle_repo::SubtitleRepo;
use crate::handlers::media::local_media::resolve_local_path;
use crate::handlers::user::AuthUser;
use crate::handlers::{err_resp, ok};
use crate::scheduler::tasks::persist_playback_progress;
use crate::services::transcode_decision::{self, ClientProfile, VideoStreamInfo};
use sea_orm::EntityTrait;

// ── Query parameters ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StreamUrlQuery {
    #[serde(default)]
    pub vc: String,
    #[serde(default)]
    pub vr: String,
    #[serde(default)]
    pub ac: String,
    #[serde(default)]
    pub containers: String,
    #[serde(rename = "h264Level")]
    pub h264_level: Option<String>,
    #[serde(rename = "hevcLevel")]
    pub hevc_level: Option<String>,
    #[serde(rename = "maxBitrate")]
    pub max_bitrate: Option<String>,
    #[serde(rename = "maxWidth")]
    pub max_width: Option<String>,
    #[serde(rename = "maxHeight")]
    pub max_height: Option<String>,
    #[serde(rename = "maxRefFrames")]
    pub max_ref_frames: Option<String>,
    #[serde(rename = "maxFramerate")]
    pub max_framerate: Option<String>,
    #[serde(rename = "forceSDR")]
    pub force_sdr: Option<String>,
    #[serde(rename = "audioIndex")]
    pub audio_index: Option<String>,
    // Jellyfin-parity additions
    #[serde(rename = "supportsAnamorphic")]
    pub supports_anamorphic: Option<String>,
    #[serde(rename = "hevcCodecTags")]
    pub hevc_codec_tags: Option<String>,
    #[serde(rename = "maxVideoBitDepth")]
    pub max_video_bit_depth: Option<String>,
    #[serde(rename = "maxAudioChannels")]
    pub max_audio_channels: Option<String>,
    #[serde(rename = "maxAudioBitrate")]
    pub max_audio_bitrate: Option<String>,
    #[serde(rename = "maxAudioSampleRate")]
    pub max_audio_sample_rate: Option<String>,
    #[serde(rename = "maxAudioBitDepth")]
    pub max_audio_bit_depth: Option<String>,
    /// Safari-specific: HEVC max framerate (60fps)
    #[serde(rename = "hevcMaxFramerate")]
    pub hevc_max_framerate: Option<String>,
    /// AV1 max level (15-19, Jellyfin: browserDeviceProfile.js)
    #[serde(rename = "av1Level")]
    pub av1_level: Option<String>,
    /// H.264 supported profiles ("high|main|baseline|constrained baseline|high 10")
    #[serde(rename = "h264Profiles")]
    pub h264_profiles: Option<String>,
    /// HEVC supported profiles ("main|main 10")
    #[serde(rename = "hevcProfiles")]
    pub hevc_profiles: Option<String>,
}

#[derive(Deserialize)]
pub struct ResumePositionQuery {
    #[serde(rename = "movieId")]
    pub movie_id: Option<String>,
    #[serde(rename = "episodeId")]
    pub episode_id: Option<String>,
}

#[derive(Deserialize)]
pub struct WatchHistoryQuery {
    #[serde(rename = "movieId")]
    pub movie_id: Option<String>,
    #[serde(rename = "episodeId")]
    pub episode_id: Option<String>,
    pub limit: Option<u64>,
}

// ── GET /api/playback/stream-url/{file_id} ───────────────────────────────────

pub async fn stream_url(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    Query(q): Query<StreamUrlQuery>,
    AuthUser(auth): AuthUser,
) -> Response {
    let file_uuid: Uuid = match file_id.parse() {
        Ok(u) => u,
        Err(_) => {
            return err_resp::<StreamUrlDto>(StatusCode::BAD_REQUEST, "Invalid file ID".into())
                .into_response();
        }
    };

    let db = &state.db;

    // Look up media file
    let file = match media_files::Entity::find_by_id(file_uuid).one(db).await {
        Ok(Some(f)) => f,
        Ok(None) => {
            return err_resp::<StreamUrlDto>(StatusCode::NOT_FOUND, "File not found".into())
                .into_response();
        }
        Err(e) => {
            return err_resp::<StreamUrlDto>(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                .into_response();
        }
    };

    // Look up source (file_systems)
    let source = if let Some(src_id) = file.source_id {
        file_systems::Entity::find_by_id(src_id)
            .one(db)
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    let client_profile = ClientProfile::parse(
        &q.vc,
        &q.vr,
        q.h264_level.as_deref(),
        q.hevc_level.as_deref(),
        q.max_bitrate.as_deref(),
        q.max_width.as_deref(),
        q.max_height.as_deref(),
        q.max_ref_frames.as_deref(),
        q.max_framerate.as_deref(),
        q.supports_anamorphic.as_deref(),
        q.hevc_codec_tags.as_deref(),
        q.max_video_bit_depth.as_deref(),
        q.max_audio_channels.as_deref(),
        q.max_audio_bitrate.as_deref(),
        q.max_audio_sample_rate.as_deref(),
        q.max_audio_bit_depth.as_deref(),
        q.hevc_max_framerate.as_deref(),
        q.av1_level.as_deref(),
        q.h264_profiles.as_deref(),
        q.hevc_profiles.as_deref(),
    );
    let force_sdr = q.force_sdr.as_deref() == Some("1");
    let client_containers: Vec<String> = if q.containers.is_empty() {
        vec![]
    } else {
        q.containers
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let client_audio_codecs: Vec<String> = if q.ac.is_empty() {
        vec![]
    } else {
        q.ac.split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    };

    // ── Filesystem source ───────────────────────────────────────────────────
    let Some(source) = source else {
        return err_resp::<StreamUrlDto>(StatusCode::BAD_REQUEST, "File has no source".into())
            .into_response();
    };

    let source_type = source.r#type.as_str();
    let is_network = transcode_decision::is_net_fs_source(source_type);

    if is_network || source_type == "local" {
        // Audio-only → direct stream
        if transcode_decision::is_audio_only_file(
            file.video_codec.as_deref(),
            file.mime_type.as_deref(),
        ) {
            let url = build_direct_stream_url(&file);
            return ok(StreamUrlDto { url }).into_response();
        }

        // Parse stream metadata
        let audio_streams = AudioStreamInfo::from_json_array(file.audio_streams.as_ref());
        let audio_index = q
            .audio_index
            .as_deref()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        let selected_audio = audio_streams.get(audio_index).or(audio_streams.first());

        let audio_reason = transcode_decision::audio_transcode_reason(
            selected_audio.map(|a| a.codec.as_str()),
            selected_audio,
            &client_profile,
            &client_audio_codecs,
        );
        let transcode_audio = audio_reason.is_some();

        let vs = VideoStreamInfo::from_json(file.video_streams.as_ref());
        let video_reason = transcode_decision::video_transcode_reason(
            file.video_codec.as_deref(),
            file.video_profile.as_deref(),
            file.hdr_type.as_deref(),
            &vs,
            &client_profile,
        );
        let transcode_video = video_reason.is_some();
        let container_reason =
            transcode_decision::container_transcode_reason(&file.path, &client_containers);
        let transcode_container = container_reason.is_some();
        let codec_tag_reason = transcode_decision::codec_tag_transcode_reason(
            file.video_codec.as_deref(),
            &vs,
            &client_profile,
            &file.path,
        );
        let transcode_codec_tag = codec_tag_reason.is_some();

        let is_hdr_content = transcode_decision::is_hdr(file.hdr_type.as_deref());
        let should_transcode_video = transcode_video || (force_sdr && is_hdr_content);
        let tonemap_opts = if should_transcode_video && is_hdr_content {
            Some(TonemapOptions {
                algorithm: "bt2390".to_string(),
                peak: 100.0,
                desat: 0.0,
                // Jellyfin default TonemappingMode is "max"
                mode: "max".to_string(),
                param: 0.0,
                range: "auto".to_string(),
            })
        } else {
            None
        };

        // Jellyfin three-way decision:
        //   DirectPlay:    container + codecs + codec tag all supported by client
        //   DirectStream:  container/audio/codec-tag issues but video ok → remux (-c:v copy)
        //   Transcode:     video codec issues → re-encode
        if transcode_audio || should_transcode_video || transcode_container || transcode_codec_tag {
            // Collect all reasons for logging
            let mut reasons = Vec::new();
            if let Some(ref r) = container_reason {
                reasons.push(r.clone());
            }
            if let Some(ref r) = codec_tag_reason {
                reasons.push(r.clone());
            }
            if let Some(ref r) = video_reason {
                reasons.push(r.clone());
            } else if force_sdr && is_hdr_content {
                reasons.push("ForceSDR (HDR→SDR tone mapping requested)".to_string());
            }
            if let Some(ref r) = audio_reason {
                reasons.push(r.clone());
            }

            let play_method = if should_transcode_video {
                "Transcode"
            } else {
                "DirectStream"
            };
            let video_desc = if should_transcode_video {
                format!("video=transcode({})", file.video_codec.as_deref().unwrap_or("?"))
            } else {
                format!("video=copy({})", file.video_codec.as_deref().unwrap_or("?"))
            };
            let audio_desc = if transcode_audio {
                format!("audio=transcode({}→aac)", selected_audio.map(|a| a.codec.as_str()).unwrap_or("?"))
            } else {
                format!("audio=copy({})", selected_audio.map(|a| a.codec.as_str()).unwrap_or("?"))
            };
            let hdr = file.hdr_type.as_deref().unwrap_or("SDR");
            info!(
                "[Playback] {} → {} | {} {} | hdr={} | reasons=[{}]",
                file.path, play_method, video_desc, audio_desc, hdr,
                reasons.join(", "),
            );

            // Create HLS session
            match create_hls_session_internal(
                &state,
                &file,
                &audio_streams,
                audio_index,
                should_transcode_video,
                transcode_audio,
                tonemap_opts,
                &vs,
                source_type == "local",
                source.config.as_ref(),
                &auth.user_id,
                file.source_id.as_ref().map(|u| u.to_string()).as_deref(),
            )
            .await
            {
                Ok(url) => return ok(StreamUrlDto { url }).into_response(),
                Err(e) => {
                    return err_resp::<StreamUrlDto>(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("HLS stream failed: {e}"),
                    )
                    .into_response();
                }
            }
        }

        // Direct play
        info!(
            "[Playback] {} → DirectPlay | video={} audio={} hdr={}",
            file.path,
            file.video_codec.as_deref().unwrap_or("?"),
            selected_audio.map(|a| a.codec.as_str()).unwrap_or("?"),
            file.hdr_type.as_deref().unwrap_or("SDR"),
        );
        let url = build_direct_stream_url(&file);
        return ok(StreamUrlDto { url }).into_response();
    }

    err_resp::<StreamUrlDto>(
        StatusCode::BAD_REQUEST,
        format!("Unsupported source type: {source_type}"),
    )
    .into_response()
}

/// Build a direct stream URL (relative to Rust server) with tracking params.
fn build_direct_stream_url(file: &media_files::Model) -> String {
    format!("/api/media-files/{}/stream", file.id)
}

/// Create an HLS transcoding session and return the playlist URL.
async fn create_hls_session_internal(
    state: &AppState,
    file: &media_files::Model,
    audio_streams: &[AudioStreamInfo],
    audio_index: usize,
    transcode_video: bool,
    transcode_audio: bool,
    tonemap: Option<TonemapOptions>,
    vs: &VideoStreamInfo,
    is_local: bool,
    source_config: Option<&serde_json::Value>,
    user_id: &str,
    source_id: Option<&str>,
) -> Result<String, String> {
    let port = std::env::var("PORT").unwrap_or_else(|_| "5678".to_string());
    let base_url = format!("http://127.0.0.1:{port}");

    let local_path = if is_local {
        Some(resolve_local_path(&file.path, source_config))
    } else {
        None
    };

    let hls_audio_streams: Vec<HlsAudioStream> = audio_streams
        .iter()
        .map(|a| HlsAudioStream {
            index: a.index as u32,
            codec: a.codec.clone(),
            channels: a.channels.map(|c| c as u32),
            language: a.language.clone(),
            title: a.title.clone(),
            bitrate: a.bitrate.map(|b| b as u32),
            sample_rate: a.sample_rate.map(|s| s as u32),
            is_default: a.is_default,
        })
        .collect();

    // Build DirectInput for remote sources — reads directly through VFS AVIO.
    // For local sources, FFmpeg reads the file directly (no AVIO needed).
    // The tap sender is passed into the read_at closure to feed VFS bytes
    // directly to the subtitle extractor — zero extra copy.
    let direct_input = if !is_local && local_path.is_none() {
        let tap = build_subtitle_tap_for_hls(state, file).await;
        let input = build_direct_input(state, source_id, &file.path, file.size, tap)
            .await
            .ok_or_else(|| {
                format!(
                    "Failed to build DirectInput for remote file {} (source={:?}, size={:?})",
                    file.id, source_id, file.size,
                )
            })?;
        Some(input)
    } else {
        None
    };

    let req = CreateSessionRequest {
        file_id: file.id.to_string(),
        local_path,
        duration_secs: file.duration.unwrap_or(0) as f64,
        audio_stream_index: audio_index as u32,
        audio_streams: hls_audio_streams,
        transcode_video: transcode_video,
        transcode_audio: Some(transcode_audio),
        tonemap,
        video_codec: file.video_codec.clone(),
        video_fps: vs.frame_rate,
        video_bitrate: vs.bitrate_kbps.map(|k| (k * 1000) as u64),
        deinterlace: vs.is_interlaced.unwrap_or(false),
        user_id: Some(user_id.to_string()),
        movie_id: file.movie_id.map(|u| u.to_string()),
        episode_id: file.episode_id.map(|u| u.to_string()),
        direct_input,
    };

    let info = state
        .hls_manager
        .create_session(req, &base_url)
        .await
        .map_err(|e| e.to_string())?;

    Ok(format!("/api/hls/{}/playlist.m3u8", info.session_id))
}

/// Build a `DirectInput` for remote VFS-backed files.
///
/// This allows FFmpeg to read directly from VFS via a custom AVIO context,
/// bypassing the HTTP→VFS→SMB round-trip that adds ~500ms per seek.
async fn build_direct_input(
    state: &AppState,
    source_id: Option<&str>,
    file_path: &str,
    file_size: Option<i64>,
    subtitle_tap: Option<tokio::sync::mpsc::Sender<(Vec<u8>, u64)>>,
) -> Option<Arc<ffmpeg_tool::DirectInput>> {
    let source_id = source_id?;
    let file_size = file_size.filter(|&s| s > 0)? as u64;

    let vfs = state.sources.ensure_vfs(source_id).await.ok()?;
    let path = file_path.to_string();
    let handle = tokio::runtime::Handle::current();

    // Extract filename for format detection hint
    let filename_hint = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from);

    let input = ffmpeg_tool::DirectInput {
        read_at: Arc::new(move |offset: u64, size: usize| {
            let read_path = std::path::Path::new(&path);
            match handle.block_on(vfs.read_bytes(read_path, offset, Some(size as u64))) {
                Ok(bytes) => {
                    // Feed VFS bytes to subtitle extractor when a tap is active.
                    // Clone only when needed — the no-tap path (common case) is zero-copy.
                    if let Some(ref tx) = subtitle_tap {
                        let _ = tx.try_send((bytes.clone(), offset));
                    }
                    Ok(bytes)
                }
                Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
            }
        }),
        size: file_size,
        filename_hint,
    };

    info!(
        "[HLS] DirectInput: {} ({}MB)",
        file_path,
        file_size / 1024 / 1024,
    );

    Some(Arc::new(input))
}

/// Build a subtitle stream tap for HLS sessions using AVIO direct input.
///
/// When FFmpeg reads via AVIO (bypassing the HTTP stream endpoint), the
/// normal subtitle extraction tap in the stream handler is never triggered.
/// This function builds an equivalent tap that can be fed from the AVIO reads.
async fn build_subtitle_tap_for_hls(
    state: &AppState,
    file: &media_files::Model,
) -> Option<tokio::sync::mpsc::Sender<(Vec<u8>, u64)>> {
    let file_id = file.id.to_string();
    let rows = SubtitleRepo::load_file_subtitles(&state.db, &file_id)
        .await
        .ok()?;
    if rows.is_empty() {
        return None;
    }

    let ffprobe_raw = rows[0].ffprobe_raw.clone();
    let start_time_ms = extract_start_time_ms(&ffprobe_raw);
    let subs: Vec<_> = rows.iter().map(|row| row.to_embedded_record()).collect();
    let tracks = resolve_subtitle_tracks(&ffprobe_raw, &subs);

    build_stream_tap(
        &state.subtitle_cache,
        &state.tap_registry,
        tracks,
        &file.path,
        start_time_ms,
    )
}

// ── DELETE /api/playback/stop-session/{file_id} (authenticated) ──────────────

pub async fn stop_session_delete(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    AuthUser(_): AuthUser,
) -> Response {
    stop_sessions_by_file(&state, &file_id).await;
    StatusCode::NO_CONTENT.into_response()
}

// ── POST /api/playback/stop-session/{file_id} (beacon, no auth) ─────────────

pub async fn stop_session_beacon(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
) -> Response {
    stop_sessions_by_file(&state, &file_id).await;
    StatusCode::NO_CONTENT.into_response()
}

/// Shared logic: stop HLS sessions for a file and persist progress.
async fn stop_sessions_by_file(state: &AppState, file_id: &str) {
    debug!("[Playback] stop-session request for file {}", file_id);
    let snapshots = state.hls_manager.playback_snapshots().await;
    let file_snapshots: Vec<_> = snapshots
        .into_iter()
        .filter(|s| s.file_id == file_id)
        .collect();
    state.hls_manager.stop_session_for_file(file_id).await;
    for snap in &file_snapshots {
        if let Err(e) = persist_playback_progress(&state.db, snap).await {
            warn!(
                "[Playback] failed to persist final progress for {}: {}",
                snap.session_id, e
            );
        }
    }
}

// ── GET /api/playback/resume-position ────────────────────────────────────────

pub async fn resume_position(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ResumePositionQuery>,
    AuthUser(auth): AuthUser,
) -> Response {
    let user_id: Uuid = match auth.user_id.parse() {
        Ok(u) => u,
        Err(_) => {
            return err_resp::<ResumePositionDto>(
                StatusCode::BAD_REQUEST,
                "Invalid user ID".into(),
            )
            .into_response();
        }
    };
    let movie_id = q.movie_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());
    let episode_id = q.episode_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());

    match PlaybackRepo::get_resume_position(&state.db, user_id, movie_id, episode_id).await {
        Ok(dto) => ok(dto).into_response(),
        Err(e) => e.into_response(),
    }
}

// ── GET /api/playback/watch-history ──────────────────────────────────────────

pub async fn watch_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<WatchHistoryQuery>,
    AuthUser(auth): AuthUser,
) -> Response {
    let user_id: Uuid = match auth.user_id.parse() {
        Ok(u) => u,
        Err(_) => {
            return err_resp::<Vec<WatchHistoryItemDto>>(
                StatusCode::BAD_REQUEST,
                "Invalid user ID".into(),
            )
            .into_response();
        }
    };
    let movie_id = q.movie_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());
    let episode_id = q.episode_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());
    let limit = q.limit.unwrap_or(20).min(50).max(1);

    match PlaybackRepo::get_watch_history(&state.db, user_id, movie_id, episode_id, limit).await {
        Ok(items) => ok(items).into_response(),
        Err(e) => e.into_response(),
    }
}
