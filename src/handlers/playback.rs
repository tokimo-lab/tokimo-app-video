use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokimo_package_hls::types::{AudioStreamInfo as HlsAudioStream, CreateSessionRequest, TonemapOptions};
use tokimo_package_subtitle::{
    resolve::{extract_start_time_ms, resolve_subtitle_tracks},
    tap_builder::build_stream_tap,
};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::db::entities::{vfs, video_files};
use crate::db::models::playback::{AudioStreamInfo, ResumePositionDto, StreamUrlDto, WatchHistoryItemDto};
use crate::db::repos::media::playback_session_repo::CreatePlaybackSessionInput;
use crate::db::repos::media::{PlaybackRepo, PlaybackSessionRepo};
use crate::db::repos::subtitle_repo::SubtitleRepo;
use crate::handlers::media::utils::resolve_local_path;
use crate::handlers::user::AuthUser;
use crate::handlers::{err_resp, ok};
use sea_orm::EntityTrait;
use tokimo_package_hls::transcode_decision::{self, ClientProfile, VideoStreamInfo};
use tokimo_package_iso::IsoMeta;

// ── Request body ──────────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

fn default_sdr() -> Vec<String> {
    vec!["SDR".to_string()]
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamUrlBody {
    #[serde(default)]
    pub video_codecs: Vec<String>,
    #[serde(default = "default_sdr")]
    pub video_range_types: Vec<String>,
    #[serde(default)]
    pub audio_codecs: Vec<String>,
    #[serde(default)]
    pub containers: Vec<String>,
    pub h264_level: Option<f64>,
    pub hevc_level: Option<i32>,
    pub max_bitrate: Option<i64>,
    pub max_width: Option<i32>,
    pub max_height: Option<i32>,
    pub max_ref_frames: Option<i32>,
    pub max_framerate: Option<f64>,
    #[serde(default = "default_true")]
    pub supports_anamorphic: bool,
    #[serde(default)]
    pub hevc_codec_tags: Vec<String>,
    pub max_video_bit_depth: Option<i32>,
    pub max_audio_channels: Option<i32>,
    pub max_audio_bitrate: Option<i64>,
    pub max_audio_sample_rate: Option<i32>,
    pub max_audio_bit_depth: Option<i32>,
    /// Safari-specific: HEVC max framerate (60fps)
    pub hevc_max_framerate: Option<f64>,
    /// AV1 max level (15-19)
    pub av1_level: Option<i32>,
    /// H.264 supported profiles ("high", "main", "baseline", ...)
    #[serde(default)]
    pub h264_profiles: Vec<String>,
    /// HEVC supported profiles ("main", "main 10", ...)
    #[serde(default)]
    pub hevc_profiles: Vec<String>,
    #[serde(rename = "forceSDR", default)]
    pub force_sdr: bool,
    pub audio_index: Option<usize>,
    /// Optional: reuse an existing watch history record (for "continue watching").
    /// If absent, the backend creates a new record.
    pub watch_history_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ResumePositionQuery {
    #[serde(rename = "videoItemId")]
    pub video_item_id: Option<String>,
    #[serde(rename = "episodeId")]
    pub episode_id: Option<String>,
}

#[derive(Deserialize)]
pub struct WatchHistoryQuery {
    #[serde(rename = "videoItemId")]
    pub video_item_id: Option<String>,
    #[serde(rename = "episodeId")]
    pub episode_id: Option<String>,
    #[serde(rename = "tvShowId")]
    pub tv_show_id: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportProgressBody {
    pub watch_history_id: String,
    pub position: f64,
    pub duration: f64,
}

// ── POST /api/playback/stream-url/{file_id} ──────────────────────────────────

#[allow(clippy::too_many_lines)]
pub async fn stream_url(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    AuthUser(auth): AuthUser,
    headers: HeaderMap,
    Json(body): Json<StreamUrlBody>,
) -> Response {
    let file_uuid: Uuid = match file_id.parse() {
        Ok(u) => u,
        Err(_) => {
            return err_resp::<StreamUrlDto>(StatusCode::BAD_REQUEST, "Invalid file ID".into()).into_response();
        }
    };

    let db = &state.db;

    // Single JOIN query: video_files + vfs (no second round-trip).
    // Fall back to music_files if not found in video_files.
    let video_row = match video_files::Entity::find_by_id(file_uuid)
        .find_also_related(vfs::Entity)
        .one(db)
        .await
    {
        Ok(row) => row,
        Err(e) => {
            return err_resp::<StreamUrlDto>(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    let Some((file, source)) = video_row else {
        return err_resp::<StreamUrlDto>(StatusCode::NOT_FOUND, "Video file not found".into()).into_response();
    };

    // ── Watch history: create or reuse ──────────────────────────────────────
    let user_uuid: Uuid = match auth.user_id.parse() {
        Ok(u) => u,
        Err(_) => {
            return err_resp::<StreamUrlDto>(StatusCode::BAD_REQUEST, "Invalid user ID".into()).into_response();
        }
    };
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let watch_history_id: Option<String> = if let Some(ref existing_id) = body.watch_history_id {
        // Reuse existing record — verify ownership
        if let Ok(hid) = existing_id.parse::<Uuid>() {
            if let Ok(true) = PlaybackRepo::verify_history_ownership(db, hid, user_uuid).await {
                Some(hid.to_string())
            } else {
                warn!("[Playback] watch_history_id {existing_id} not owned by user, creating new");
                match PlaybackRepo::create_history(db, user_uuid, file_uuid, user_agent.clone(), file.duration).await {
                    Ok(id) => Some(id.to_string()),
                    Err(e) => {
                        warn!("[Playback] failed to create watch history: {e}");
                        None
                    }
                }
            }
        } else {
            None
        }
    } else {
        // No existing ID — create new record
        match PlaybackRepo::create_history(db, user_uuid, file_uuid, user_agent.clone(), file.duration).await {
            Ok(id) => Some(id.to_string()),
            Err(e) => {
                warn!("[Playback] failed to create watch history: {e}");
                None
            }
        }
    };

    let client_profile = ClientProfile {
        supported_vc: body.video_codecs.iter().map(|s| s.trim().to_lowercase()).collect(),
        supported_range_types: if body.video_range_types.is_empty() {
            vec!["SDR".to_string()]
        } else {
            body.video_range_types.clone()
        },
        max_h264_level: body.h264_level,
        max_hevc_level: body.hevc_level,
        max_bitrate: body.max_bitrate,
        max_width: body.max_width,
        max_height: body.max_height,
        max_ref_frames: body.max_ref_frames,
        max_framerate: body.max_framerate,
        supports_anamorphic: body.supports_anamorphic,
        hevc_codec_tags: body.hevc_codec_tags.iter().map(|s| s.trim().to_lowercase()).collect(),
        max_video_bit_depth: body.max_video_bit_depth,
        max_audio_channels: body.max_audio_channels,
        max_audio_bitrate: body.max_audio_bitrate,
        max_audio_sample_rate: body.max_audio_sample_rate,
        max_audio_bit_depth: body.max_audio_bit_depth,
        hevc_max_framerate: body.hevc_max_framerate,
        max_av1_level: body.av1_level,
        h264_profiles: body
            .h264_profiles
            .iter()
            .map(|s| s.trim().to_lowercase().replace(' ', ""))
            .collect(),
        hevc_profiles: body
            .hevc_profiles
            .iter()
            .map(|s| s.trim().to_lowercase().replace(' ', ""))
            .collect(),
    };
    let force_sdr = body.force_sdr;
    let client_containers: Vec<String> = body.containers.iter().map(|s| s.trim().to_lowercase()).collect();
    let client_audio_codecs: Vec<String> = body.audio_codecs.iter().map(|s| s.trim().to_lowercase()).collect();

    // ── Filesystem source ───────────────────────────────────────────────────
    let Some(source) = source else {
        return err_resp::<StreamUrlDto>(StatusCode::BAD_REQUEST, "File has no source".into()).into_response();
    };

    let source_type = source.r#type.as_str();
    if source_type != "local" && !transcode_decision::is_net_fs_source(source_type) {
        return err_resp::<StreamUrlDto>(
            StatusCode::BAD_REQUEST,
            format!("Unsupported source type: {source_type}"),
        )
        .into_response();
    }

    // ISO disc images must always go through HLS — they can't be direct-played.
    // Check BEFORE is_audio_only_file: un-scanned ISOs have video_codec=NULL which
    // would otherwise be mis-classified as audio-only.
    let is_iso = file.mime_type.as_deref() == Some("video/iso-image") || file.path.to_lowercase().ends_with(".iso");
    let iso_type: Option<&'static str> = if is_iso {
        Some(detect_iso_type(&file.path))
    } else {
        None
    };

    // Audio-only → direct stream (skip for ISO which has no video_codec in DB)
    if !is_iso && transcode_decision::is_audio_only_file(file.video_codec.as_deref(), file.mime_type.as_deref()) {
        let url = build_direct_stream_url(&file);
        return ok(StreamUrlDto {
            url,
            watch_history_id: watch_history_id.clone(),
        })
        .into_response();
    }

    // Parse stream metadata
    let audio_streams = AudioStreamInfo::from_json_array(file.audio_streams.as_ref());
    let audio_index = body.audio_index.unwrap_or(0);
    let selected_audio = audio_streams.get(audio_index).or(audio_streams.first());
    let selected_audio_info = selected_audio.map(|a| tokimo_package_hls::transcode_decision::AudioInfo {
        channels: a.channels,
        bitrate: a.bitrate,
        sample_rate: a.sample_rate,
        bit_depth: a.bit_depth,
        profile: a.profile.clone(),
    });

    let audio_reason = transcode_decision::audio_transcode_reason(
        selected_audio.map(|a| a.codec.as_str()),
        selected_audio_info.as_ref(),
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
    let container_reason = transcode_decision::container_transcode_reason(&file.path, &client_containers);
    let transcode_container = container_reason.is_some();
    let codec_tag_reason =
        transcode_decision::codec_tag_transcode_reason(file.video_codec.as_deref(), &vs, &client_profile, &file.path);
    let transcode_codec_tag = codec_tag_reason.is_some();

    let is_hdr_content = transcode_decision::is_hdr(file.hdr_type.as_deref());
    let open_gop_reason = transcode_decision::open_gop_transcode_reason(&file.path, file.video_codec.as_deref());
    let should_transcode_video = transcode_video || (force_sdr && is_hdr_content) || open_gop_reason.is_some();

    // AV1 and VP9 cannot be properly muxed into MPEG-TS segments (the container
    // used for DirectStream / copy mode). If HLS is already required for any other
    // reason (audio transcode, container, codec-tag, ISO), force video transcode so
    // the pipeline switches to fMP4 and re-encodes to H.264/HEVC instead.
    let mpegts_incompat_reason: Option<String> =
        if !should_transcode_video && (transcode_audio || transcode_container || transcode_codec_tag || is_iso) {
            let raw = file.video_codec.as_deref().unwrap_or("").to_lowercase();
            if raw.contains("av1") || raw.contains("vp9") || raw.contains("vp8") {
                Some(format!("VideoCodecNotCompatibleWithMpegTs ({raw})"))
            } else {
                None
            }
        } else {
            None
        };
    let should_transcode_video = should_transcode_video || mpegts_incompat_reason.is_some();

    // Mediabunny (client-side AC3/EAC3 decoder) is only active in direct-stream
    // mode. When HLS is forced (container remux, video transcode, ISO, codec-tag),
    // the frontend falls into `isHLS` path which disables mediabunny — the browser
    // would have to decode AC3 natively, which it cannot. Force AAC transcode.
    let hls_audio_compat_reason: Option<String> =
        if !transcode_audio && (is_iso || should_transcode_video || transcode_container || transcode_codec_tag) {
            let codec = selected_audio.map(|a| a.codec.to_lowercase()).unwrap_or_default();
            match codec.as_str() {
                "ac3" | "eac3" => Some(format!(
                    "AudioNotCompatibleWithHls ({codec}) — mediabunny unavailable in HLS mode"
                )),
                _ => None,
            }
        } else {
            None
        };
    let transcode_audio = transcode_audio || hls_audio_compat_reason.is_some();
    let audio_reason = audio_reason.or(hls_audio_compat_reason);

    // ── Target audio codec selection ───────────────────────────────────────
    // Mirrors Jellyfin's TranscodingProfile.AudioCodec priority: iterate the
    // client's ordered codec list, filter by HLS container constraints, pick first.
    // fmp4 (used when video is transcoded) supports more codecs than mpegts.
    let hls_audio_allowed: &[&str] = if should_transcode_video {
        &["aac", "ac3", "eac3", "mp3", "alac", "flac", "opus"]
    } else {
        &["aac", "ac3", "eac3", "mp3"]
    };
    let target_audio_codec: Option<String> = if transcode_audio {
        client_audio_codecs
            .iter()
            .find(|c| hls_audio_allowed.contains(&c.as_str()))
            .cloned()
            .or_else(|| Some("aac".to_string()))
    } else {
        None
    };

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
    // ISO images always require transcoding (libbluray / UDF reader path).
    let needs_hls = is_iso || transcode_audio || should_transcode_video || transcode_container || transcode_codec_tag;

    let play_method = if should_transcode_video {
        "Transcode"
    } else if needs_hls {
        "DirectStream"
    } else {
        "DirectPlay"
    };

    {
        let filename = std::path::Path::new(&file.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&file.path);
        let eff_video_reason = video_reason
            .as_deref()
            .or(if force_sdr && is_hdr_content {
                Some("ForceSDR (HDR→SDR tone mapping)")
            } else {
                None
            })
            .or(mpegts_incompat_reason.as_deref())
            .or(if should_transcode_video {
                open_gop_reason.as_deref()
            } else {
                None
            });
        let eff_remux_reason = if should_transcode_video {
            None
        } else {
            open_gop_reason.as_deref()
        };
        let audio_codec_str = selected_audio.map_or("", |a| a.codec.as_str());
        let audio_ch_str = selected_audio
            .and_then(|a| a.channels)
            .map(|c| c.to_string())
            .unwrap_or_default();
        let client_prefers_hevc = client_profile.supported_vc.iter().any(|c| c == "hevc");
        let target_video_codec = if client_prefers_hevc { "hevc" } else { "h264" };
        let target_audio_codec_log = target_audio_codec.as_deref().unwrap_or("aac");
        info!(
            target: "playback::decision",
            filename = %filename,
            video_codec = %file.video_codec.as_deref().unwrap_or(""),
            video_profile = %file.video_profile.as_deref().unwrap_or(""),
            hdr = %file.hdr_type.as_deref().unwrap_or("SDR"),
            audio_codec = %audio_codec_str,
            audio_ch = %audio_ch_str,
            audio_idx = %audio_index,
            method = %play_method,
            transcode_video = should_transcode_video,
            transcode_audio = transcode_audio,
            target_video_codec = %target_video_codec,
            target_audio_codec = %target_audio_codec_log,
            reason_video = %eff_video_reason.unwrap_or(""),
            reason_audio = %audio_reason.as_deref().unwrap_or(""),
            reason_container = %container_reason.as_deref().unwrap_or(""),
            reason_codec_tag = %codec_tag_reason.as_deref().unwrap_or(""),
            reason_remux = %eff_remux_reason.unwrap_or(""),
            ""
        );
    }

    // Record the active playback session.
    {
        let user_uuid = auth.user_id.parse::<Uuid>().ok();
        let session_uuid = auth.session_id.parse::<Uuid>().ok();
        if let Some(uid) = user_uuid {
            let db_clone = state.db.clone();
            let ua = headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);

            let transcode_video_codec = if should_transcode_video {
                let client_prefers_hevc = client_profile.supported_vc.iter().any(|c| c == "hevc");
                Some(if client_prefers_hevc {
                    "hevc".to_string()
                } else {
                    "h264".to_string()
                })
            } else {
                None
            };

            let media_streams = {
                let mut streams = serde_json::json!({});
                if let Some(v) = file.video_streams.as_ref() {
                    streams["video"] = v.clone();
                }
                if let Some(a) = file.audio_streams.as_ref() {
                    streams["audio"] = a.clone();
                }
                streams
            };

            let transcode_reasons = serde_json::json!({
                "video": video_reason,
                "audio": audio_reason,
                "container": container_reason,
                "codecTag": codec_tag_reason,
            });

            let caps = serde_json::to_value(&body).ok();

            let input = CreatePlaybackSessionInput {
                user_id: uid,
                session_id: session_uuid,
                file_id: file_uuid,
                client_name: Some("Tokimo Web".to_string()),
                user_agent: ua,
                play_method: play_method.to_string(),
                source_container: std::path::Path::new(&file.path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase),
                source_video_codec: file.video_codec.clone(),
                source_video_profile: file.video_profile.clone(),
                source_hdr_type: file.hdr_type.clone(),
                source_width: file.video_width,
                source_height: file.video_height,
                source_duration: file.duration,
                source_file_size: file.size,
                transcode_video_codec,
                transcode_audio_codec: target_audio_codec.clone(),
                transcode_reasons: Some(transcode_reasons),
                media_streams_raw: Some(media_streams),
                client_capabilities: caps,
            };

            tokio::spawn(async move {
                if let Err(e) = PlaybackSessionRepo::create(&db_clone, input).await {
                    warn!("[Playback] failed to record playback session: {e}");
                }
            });
        }
    }

    if needs_hls {
        // Create HLS session
        match create_hls_session_internal(
            &state,
            &file,
            &audio_streams,
            audio_index,
            should_transcode_video,
            transcode_audio,
            target_audio_codec,
            tonemap_opts,
            &vs,
            source_type == "local",
            source.config.as_ref(),
            &auth.user_id,
            file.source_id.as_ref().map(std::string::ToString::to_string).as_deref(),
            iso_type,
            client_profile.supported_vc.iter().any(|c| c == "hevc"),
        )
        .await
        {
            Ok(url) => {
                return ok(StreamUrlDto {
                    url,
                    watch_history_id: watch_history_id.clone(),
                })
                .into_response();
            }
            Err(e) => {
                return err_resp::<StreamUrlDto>(StatusCode::INTERNAL_SERVER_ERROR, format!("HLS stream failed: {e}"))
                    .into_response();
            }
        }
    }

    let url = build_direct_stream_url(&file);
    ok(StreamUrlDto { url, watch_history_id }).into_response()
}

/// Build a direct stream URL (relative to Rust server) with tracking params.
fn build_direct_stream_url(file: &video_files::Model) -> String {
    format!("/api/apps/video/files/{}/stream", file.id)
}

/// Detect whether an ISO file is a Blu-ray or DVD image from path heuristics.
///
/// Strategy:
/// 1. Path contains "dvd" (case-insensitive) → DVD ISO
/// 2. Path contains "bluray" or "blu-ray" → Blu-ray ISO
/// 3. Path contains Blu-ray quality markers (`TrueHD`, HEVC Remux, etc.) → Blu-ray
/// 4. Default: Blu-ray (the overwhelmingly common modern ISO format)
fn detect_iso_type(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.contains("dvd") {
        return "dvd";
    }
    if lower.contains("bluray") || lower.contains("blu-ray") || lower.contains("bdmv") {
        return "bluray";
    }
    "bluray" // modern ISOs are almost always Blu-ray
}

/// Thin adapter that resolves a VFS from `AppState` then defers to the
/// `tokimo_package_iso` implementation. Kept in this module so the call site
/// inside `create_hls_session_internal` doesn't need to thread `ensure_vfs`
/// through itself.
async fn build_iso_m2ts_input_from_state(
    state: &AppState,
    source_id: Option<&str>,
    file_path: &str,
    file_size: Option<i64>,
    iso_meta: Option<&IsoMeta>,
    subtitle_tap: Option<tokio::sync::mpsc::Sender<(bytes::Bytes, u64)>>,
) -> Result<Arc<tokimo_package_ffmpeg::DirectInput>, String> {
    let source_id = source_id.ok_or("ISO file has no source ID")?;
    let file_size = file_size.filter(|&s| s > 0).ok_or("ISO file has unknown size")? as u64;
    let vfs = state
        .sources
        .ensure_vfs(source_id)
        .await
        .map_err(|e| format!("Failed to get VFS for ISO source: {e}"))?;
    tokimo_package_iso::build_iso_m2ts_input(vfs, file_path, file_size, iso_meta, subtitle_tap).await
}

#[allow(clippy::too_many_arguments)]
async fn create_hls_session_internal(
    state: &AppState,
    file: &video_files::Model,
    audio_streams: &[AudioStreamInfo],
    audio_index: usize,
    transcode_video: bool,
    transcode_audio: bool,
    target_audio_codec: Option<String>,
    tonemap: Option<TonemapOptions>,
    vs: &VideoStreamInfo,
    is_local: bool,
    source_config: Option<&serde_json::Value>,
    user_id: &str,
    source_id: Option<&str>,
    iso_type: Option<&str>,
    client_supports_hevc: bool,
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
    // For remote Blu-ray ISOs, we parse the UDF filesystem first to locate the
    // main M2TS stream within the ISO, then create a DirectInput that maps
    // M2TS-local offsets to the correct byte ranges inside the ISO file.
    let (direct_input, effective_iso_type) = if !is_local && local_path.is_none() {
        if iso_type == Some("bluray") {
            // Build subtitle tap with `.m2ts` path hint so TsStreamTap is used for PGS.
            let m2ts_hint = {
                let p = &file.path;
                let lower = p.to_ascii_lowercase();
                if std::path::Path::new(&lower)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("iso"))
                {
                    format!("{}.m2ts", &p[..p.len() - 4])
                } else {
                    format!("{p}.m2ts")
                }
            };
            let tap = build_subtitle_tap_for_hls_with_path(state, file, &m2ts_hint).await;
            // Deserialize pre-scanned M2TS location info — avoids re-parsing UDF over SMB.
            let iso_meta = file
                .iso_meta
                .as_ref()
                .and_then(|v| serde_json::from_value::<IsoMeta>(v.clone()).ok());
            match build_iso_m2ts_input_from_state(state, source_id, &file.path, file.size, iso_meta.as_ref(), tap).await
            {
                Ok(input) => {
                    info!("[ISO] Remote Blu-ray ISO: M2TS extracted via UDF → AVIO ready");
                    // We serve raw M2TS bytes, so no ISO-specific FFmpeg prefix needed.
                    (Some(input), None)
                }
                Err(e) => {
                    return Err(format!(
                        "Failed to parse Blu-ray ISO UDF structure: {e}. \
                         Remote ISO playback requires the UDF filesystem to be readable."
                    ));
                }
            }
        } else {
            // Non-ISO remote file (or DVD ISO — fall back to raw AVIO).
            let tap = build_subtitle_tap_for_hls(state, file).await;
            let input = build_direct_input(state, source_id, &file.path, file.size, tap)
                .await
                .ok_or_else(|| {
                    format!(
                        "Failed to build DirectInput for remote file {} (source={:?}, size={:?})",
                        file.id, source_id, file.size,
                    )
                })?;
            (Some(input), iso_type.map(String::from).as_deref().map(|_| "dvd"))
        }
    } else {
        // Local file (including local ISO): use the filesystem path directly.
        // For local Blu-ray ISO the `iso_type` value triggers `bluray:path` in ffmpeg.rs.
        (None, iso_type)
    };

    let req = CreateSessionRequest {
        file_id: file.id.to_string(),
        local_path,
        duration_secs: f64::from(file.duration.unwrap_or(0)),
        audio_stream_index: audio_index as u32,
        audio_streams: hls_audio_streams,
        transcode_video,
        transcode_audio: Some(transcode_audio),
        target_audio_codec,
        tonemap,
        video_codec: file.video_codec.clone(),
        video_width: vs.width.map(|w| w as u32),
        video_height: vs.height.map(|h| h as u32),
        video_fps: vs.frame_rate,
        video_bitrate: vs.bitrate_kbps.map(|k| (k * 1000) as u64),
        deinterlace: vs.is_interlaced.unwrap_or(false),
        client_supports_hevc,
        user_id: Some(user_id.to_string()),
        iso_type: effective_iso_type.map(String::from),
        direct_input,
        video_item_id: None,
        episode_id: None,
    };

    let info = state
        .hls_manager
        .create_session(req, &base_url)
        .await
        .map_err(|e| e.clone())?;

    // Register this HLS session in the unified StreamSessionManager.
    // The bridge task forwards cancel() → hls_manager.stop_session_for_file(),
    // covering browser-crash / cleanup_stale cases where explicit stop-session
    // is never called. Explicit stop still goes through stop_sessions_by_file()
    // which cancels the token (firing this bridge) then persists progress.
    {
        let file_id = file.id.to_string();
        let cancel = state.stream_sessions.create_or_get(&file_id);
        let hls_manager = Arc::clone(&state.hls_manager);
        tokio::spawn(async move {
            cancel.cancelled().await;
            debug!("[StreamSession] HLS bridge fired for file_id={}", file_id);
            hls_manager.stop_session_for_file(&file_id).await;
        });
    }

    Ok(format!("/api/apps/video/hls/{}/playlist.m3u8", info.session_id))
}

/// Build a `DirectInput` for remote VFS-backed files.
///
/// This allows `FFmpeg` to read directly from VFS via a custom AVIO context,
/// bypassing the HTTP→VFS→SMB round-trip that adds ~500ms per seek.
async fn build_direct_input(
    state: &AppState,
    source_id: Option<&str>,
    file_path: &str,
    file_size: Option<i64>,
    subtitle_tap: Option<tokio::sync::mpsc::Sender<(bytes::Bytes, u64)>>,
) -> Option<Arc<tokimo_package_ffmpeg::DirectInput>> {
    let source_id = source_id?;
    let file_size = file_size.filter(|&s| s > 0)? as u64;

    let vfs = state.sources.ensure_vfs(source_id).await.ok()?;

    // Extract filename for format detection hint
    let filename_hint = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from);

    let ra = vfs.to_read_at(std::path::Path::new(file_path)).await;

    let input = if let Some(tap) = subtitle_tap {
        // Wrap ReadAt with subtitle tapping
        let inner_ra = ra;
        let tapped_ra: tokimo_vfs::ReadAt = Arc::new(move |offset: u64, size: usize| {
            let buf = inner_ra(offset, size)?;
            let shared = bytes::Bytes::from(buf);
            let _ = tap.try_send((shared.clone(), offset));
            Ok(shared.to_vec())
        });
        tokimo_package_ffmpeg::DirectInput::from_read_at(
            tapped_ra,
            file_size,
            filename_hint,
            Some(tokimo_package_ffmpeg::READAHEAD_HLS),
        )
    } else {
        tokimo_package_ffmpeg::DirectInput::from_read_at(
            ra,
            file_size,
            filename_hint,
            Some(tokimo_package_ffmpeg::READAHEAD_HLS),
        )
    };

    info!("[HLS] DirectInput: {} ({}MB)", file_path, file_size / 1024 / 1024,);

    Some(input)
}

/// Build a subtitle stream tap for HLS sessions using AVIO direct input.
///
/// When `FFmpeg` reads via AVIO (bypassing the HTTP stream endpoint), the
/// normal subtitle extraction tap in the stream handler is never triggered.
/// This function builds an equivalent tap that can be fed from the AVIO reads.
async fn build_subtitle_tap_for_hls(
    state: &AppState,
    file: &video_files::Model,
) -> Option<tokio::sync::mpsc::Sender<(bytes::Bytes, u64)>> {
    build_subtitle_tap_impl(state, file, &file.path).await
}

/// Same as `build_subtitle_tap_for_hls` but uses `tap_path` for extension
/// detection and registry keying instead of `file.path`.  Used for ISO
/// Blu-ray where the inner stream is `.m2ts` even though the file is `.iso`.
async fn build_subtitle_tap_for_hls_with_path(
    state: &AppState,
    file: &video_files::Model,
    tap_path: &str,
) -> Option<tokio::sync::mpsc::Sender<(bytes::Bytes, u64)>> {
    build_subtitle_tap_impl(state, file, tap_path).await
}

async fn build_subtitle_tap_impl(
    state: &AppState,
    file: &video_files::Model,
    tap_path: &str,
) -> Option<tokio::sync::mpsc::Sender<(bytes::Bytes, u64)>> {
    let file_id = file.id.to_string();
    let rows = SubtitleRepo::load_file_subtitles(&state.db, &file_id).await.ok()?;
    if rows.is_empty() {
        return None;
    }

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
        tap_path,
        &file_id,
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

pub async fn stop_session_beacon(State(state): State<Arc<AppState>>, Path(file_id): Path<String>) -> Response {
    stop_sessions_by_file(&state, &file_id).await;
    StatusCode::NO_CONTENT.into_response()
}

/// Shared logic: stop HLS sessions for a file.
async fn stop_sessions_by_file(state: &AppState, file_id: &str) {
    info!("[Playback] stop-session file_id={}", file_id);

    // Cancel all in-flight stream tasks (VFS + tee tasks) for this file.
    state.stream_sessions.cancel(file_id);

    // Release the subtitle tap entry, freeing its internal data buffers immediately.
    let released = state.tap_registry.release(file_id);
    if released {
        debug!("[Playback] tap registry released file_id={}", file_id);
    }

    state.hls_manager.stop_session_for_file(file_id).await;

    // Mark active playback sessions as stopped.
    if let Ok(file_uuid) = file_id.parse::<Uuid>()
        && let Err(e) = PlaybackSessionRepo::stop_by_file(&state.db, file_uuid).await
    {
        warn!("[Playback] failed to stop playback session for file {file_id}: {e}");
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
            return err_resp::<ResumePositionDto>(StatusCode::BAD_REQUEST, "Invalid user ID".into()).into_response();
        }
    };
    let video_item_id = q.video_item_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());
    let episode_id = q.episode_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());

    match PlaybackRepo::get_resume_position(&state.db, user_id, video_item_id, episode_id).await {
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
            return err_resp::<Vec<WatchHistoryItemDto>>(StatusCode::BAD_REQUEST, "Invalid user ID".into())
                .into_response();
        }
    };
    let video_item_id = q.video_item_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());
    let episode_id = q.episode_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());
    let tv_show_id = q.tv_show_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());
    let limit = q.limit.unwrap_or(20).clamp(1, 50);

    match PlaybackRepo::get_watch_history(&state.db, user_id, video_item_id, episode_id, tv_show_id, limit).await {
        Ok(items) => ok(items).into_response(),
        Err(e) => e.into_response(),
    }
}

// ── DELETE /api/playback/watch-history/{id} ──────────────────────────────────

pub async fn delete_watch_history(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    AuthUser(auth): AuthUser,
) -> Response {
    let user_id: Uuid = match auth.user_id.parse() {
        Ok(u) => u,
        Err(_) => {
            return err_resp::<()>(StatusCode::BAD_REQUEST, "Invalid user ID".into()).into_response();
        }
    };
    let history_id: Uuid = match id.parse() {
        Ok(u) => u,
        Err(_) => {
            return err_resp::<()>(StatusCode::BAD_REQUEST, "Invalid history ID".into()).into_response();
        }
    };
    match PlaybackRepo::delete_watch_history(&state.db, user_id, history_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => e.into_response(),
    }
}

// ── POST /api/playback/progress ──────────────────────────────────────────────

pub async fn report_progress(
    State(state): State<Arc<AppState>>,
    AuthUser(auth): AuthUser,
    Json(body): Json<ReportProgressBody>,
) -> Response {
    let user_id: Uuid = match auth.user_id.parse() {
        Ok(u) => u,
        Err(_) => {
            return err_resp::<()>(StatusCode::BAD_REQUEST, "Invalid user ID".into()).into_response();
        }
    };
    let history_id: Uuid = match body.watch_history_id.parse() {
        Ok(u) => u,
        Err(_) => {
            return err_resp::<()>(StatusCode::BAD_REQUEST, "Invalid watch history ID".into()).into_response();
        }
    };

    let position = body.position as i32;
    let duration = if body.duration > 0.0 {
        Some(body.duration as i32)
    } else {
        None
    };

    match PlaybackRepo::report_progress(&state.db, user_id, history_id, position, duration).await {
        Ok(_completed) => ok(()).into_response(),
        Err(e) => e.into_response(),
    }
}
