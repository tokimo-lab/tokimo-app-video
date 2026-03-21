use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use rust_hls::types::{AudioStreamInfo as HlsAudioStream, CreateSessionRequest, TonemapOptions};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::db::entities::{file_systems, media_files, media_servers};
use crate::db::models::playback::{AudioStreamInfo, ResumePositionDto, StreamUrlDto, WatchHistoryItemDto};
use crate::db::repos::media::PlaybackRepo;
use crate::db::repos::settings_repo::SettingsRepo;
use crate::handlers::user::extract_session_auth;
use crate::handlers::{err_resp, ok};
use crate::scheduler::tasks::persist_playback_progress;
use crate::services::transcode_decision::{
    self, ClientProfile, VideoStreamInfo,
};
use crate::AppState;
use sea_orm::EntityTrait;

// ── Query parameters ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StreamUrlQuery {
    #[serde(default)]
    pub vc: String,
    #[serde(default)]
    pub vr: String,
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
    headers: HeaderMap,
) -> Response {
    let auth = match extract_session_auth(&state.db, &headers).await {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };

    let file_uuid: Uuid = match file_id.parse() {
        Ok(u) => u,
        Err(_) => return err_resp::<StreamUrlDto>(StatusCode::BAD_REQUEST, "Invalid file ID".into()).into_response(),
    };

    let db = &state.db;

    // Look up media file
    let file = match media_files::Entity::find_by_id(file_uuid).one(db).await {
        Ok(Some(f)) => f,
        Ok(None) => return err_resp::<StreamUrlDto>(StatusCode::NOT_FOUND, "File not found".into()).into_response(),
        Err(e) => return err_resp::<StreamUrlDto>(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    // Look up media server (if any)
    let media_server = if let Some(ms_id) = file.media_server_id {
        media_servers::Entity::find_by_id(ms_id).one(db).await.ok().flatten()
    } else {
        None
    };

    // Look up source (file_systems)
    let source = if let Some(src_id) = file.source_id {
        file_systems::Entity::find_by_id(src_id).one(db).await.ok().flatten()
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
    );
    let force_sdr = q.force_sdr.as_deref() == Some("1");

    // ── Media server (Plex / Emby / Jellyfin) ──────────────────────────────
    if let Some(ms) = media_server {
        let ms_type = ms.r#type.as_str();
        let base_url = ms.url.trim_end_matches('/').to_string();

        if ms_type == "plex" {
            let Some(token) = &ms.token else {
                return err_resp::<StreamUrlDto>(StatusCode::INTERNAL_SERVER_ERROR, "Plex server not configured".into()).into_response();
            };
            let Some(stream_key) = &file.stream_key else {
                return err_resp::<StreamUrlDto>(StatusCode::NOT_FOUND, "No stream key for this file".into()).into_response();
            };
            let key = if stream_key.starts_with('/') { stream_key.clone() } else { format!("/{stream_key}") };
            let url = format!("{base_url}{key}?X-Plex-Token={token}&download=1");
            return ok(StreamUrlDto { url }).into_response();
        }

        if ms_type == "emby" || ms_type == "jellyfin" {
            let Some(api_key) = &ms.api_key else {
                return err_resp::<StreamUrlDto>(StatusCode::INTERNAL_SERVER_ERROR, format!("{ms_type} server not configured")).into_response();
            };
            let Some(stream_key) = &file.stream_key else {
                return err_resp::<StreamUrlDto>(StatusCode::NOT_FOUND, "No stream key for this file".into()).into_response();
            };
            let encoded_key = urlencoding::encode(stream_key);
            let encoded_api_key = urlencoding::encode(api_key);
            let url = format!("{base_url}/Videos/{encoded_key}/stream?api_key={encoded_api_key}&static=true");
            return ok(StreamUrlDto { url }).into_response();
        }

        return err_resp::<StreamUrlDto>(StatusCode::BAD_REQUEST, format!("Unsupported media server type: {ms_type}")).into_response();
    }

    // ── Filesystem source ───────────────────────────────────────────────────
    let Some(source) = source else {
        return err_resp::<StreamUrlDto>(StatusCode::BAD_REQUEST, "File has no source".into()).into_response();
    };

    let source_type = source.r#type.as_str();
    let is_network = transcode_decision::is_net_fs_source(source_type);

    if is_network || source_type == "local" {
        // Audio-only → direct stream
        if transcode_decision::is_audio_only_file(file.video_codec.as_deref(), file.mime_type.as_deref()) {
            let url = build_direct_stream_url(&file, &auth.user_id);
            return ok(StreamUrlDto { url }).into_response();
        }

        // Parse stream metadata
        let audio_streams = AudioStreamInfo::from_json_array(file.audio_streams.as_ref());
        let audio_index = q.audio_index.as_deref().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
        let selected_audio = audio_streams.get(audio_index).or(audio_streams.first());

        let transcode_audio = transcode_decision::needs_audio_transcode(
            selected_audio.map(|a| a.codec.as_str()),
        );

        let vs = VideoStreamInfo::from_json(file.video_streams.as_ref());
        let transcode_video = transcode_decision::needs_video_transcode(
            file.video_codec.as_deref(),
            file.video_profile.as_deref(),
            file.hdr_type.as_deref(),
            &file.path,
            &vs,
            &client_profile,
        );
        let transcode_container = transcode_decision::needs_container_transcode(&file.path);

        let is_hdr_content = transcode_decision::is_hdr(file.hdr_type.as_deref());
        let should_transcode_video = transcode_video || (force_sdr && is_hdr_content);
        let tonemap_opts = if should_transcode_video && is_hdr_content {
            Some(TonemapOptions {
                algorithm: "bt2390".to_string(),
                peak: 100.0,
                desat: 0.0,
            })
        } else {
            None
        };

        if transcode_audio || should_transcode_video || transcode_container {
            // Create HLS session
            match create_hls_session_internal(
                &state,
                &file,
                &audio_streams,
                audio_index,
                should_transcode_video,
                tonemap_opts,
                &vs,
                source_type == "local",
                &auth.user_id,
            ).await {
                Ok(url) => return ok(StreamUrlDto { url }).into_response(),
                Err(e) => return err_resp::<StreamUrlDto>(StatusCode::INTERNAL_SERVER_ERROR, format!("HLS stream failed: {e}")).into_response(),
            }
        }

        // Direct play
        let url = build_direct_stream_url(&file, &auth.user_id);
        return ok(StreamUrlDto { url }).into_response();
    }

    err_resp::<StreamUrlDto>(StatusCode::BAD_REQUEST, format!("Unsupported source type: {source_type}")).into_response()
}

/// Build a direct stream URL (relative to Rust server) with tracking params.
fn build_direct_stream_url(file: &media_files::Model, user_id: &str) -> String {
    let base = format!("/api/media-files/{}/stream", file.id);
    let mut params = vec![format!("dpUser={}", urlencoding::encode(user_id))];
    if let Some(mid) = &file.movie_id {
        params.push(format!("dpMovie={mid}"));
    }
    if let Some(eid) = &file.episode_id {
        params.push(format!("dpEpisode={eid}"));
    }
    if let Some(dur) = file.duration {
        params.push(format!("dpDur={dur}"));
    }
    if let Some(size) = file.size {
        params.push(format!("dpSize={size}"));
    }
    format!("{base}?{}", params.join("&"))
}

/// Create an HLS transcoding session and return the playlist URL.
async fn create_hls_session_internal(
    state: &AppState,
    file: &media_files::Model,
    audio_streams: &[AudioStreamInfo],
    audio_index: usize,
    transcode_video: bool,
    tonemap: Option<TonemapOptions>,
    vs: &VideoStreamInfo,
    is_local: bool,
    user_id: &str,
) -> Result<String, String> {
    // Get internal stream access token
    let internal_token = SettingsRepo::get_internal_stream_token(&state.db)
        .await
        .ok()
        .flatten();

    let port = std::env::var("PORT").unwrap_or_else(|_| "5678".to_string());
    let base_url = format!("http://127.0.0.1:{port}");

    let mut input_url = format!("{base_url}/api/media-files/{}/stream", file.id);
    if let Some(ref token) = internal_token {
        input_url.push_str(&format!("?accessToken={}", urlencoding::encode(token)));
    }

    let local_path = if is_local { Some(file.path.clone()) } else { None };

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

    let req = CreateSessionRequest {
        file_id: file.id.to_string(),
        input_url,
        local_path,
        duration_secs: file.duration.unwrap_or(0) as f64,
        audio_stream_index: audio_index as u32,
        audio_streams: hls_audio_streams,
        transcode_video: transcode_video,
        tonemap,
        video_codec: file.video_codec.clone(),
        video_fps: vs.frame_rate,
        video_bitrate: vs.bitrate_kbps.map(|k| (k * 1000) as u64),
        deinterlace: vs.is_interlaced.unwrap_or(false),
        user_id: Some(user_id.to_string()),
        movie_id: file.movie_id.map(|u| u.to_string()),
        episode_id: file.episode_id.map(|u| u.to_string()),
    };

    let info = state
        .hls_manager
        .create_session(req, &base_url)
        .await
        .map_err(|e| e.to_string())?;

    Ok(format!("/api/hls/{}/playlist.m3u8", info.session_id))
}

// ── DELETE /api/playback/stop-session/{file_id} (authenticated) ──────────────

pub async fn stop_session_delete(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(e) = extract_session_auth(&state.db, &headers).await {
        return e.into_response();
    }
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
    info!("[Playback] stop-session request for file {}", file_id);
    let snapshots = state.hls_manager.playback_snapshots().await;
    let file_snapshots: Vec<_> = snapshots.into_iter().filter(|s| s.file_id == file_id).collect();
    state.hls_manager.stop_session_for_file(file_id).await;
    for snap in &file_snapshots {
        if let Err(e) = persist_playback_progress(&state.db, snap).await {
            warn!("[Playback] failed to persist final progress for {}: {}", snap.session_id, e);
        }
    }
}

// ── GET /api/playback/resume-position ────────────────────────────────────────

pub async fn resume_position(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ResumePositionQuery>,
    headers: HeaderMap,
) -> Response {
    let auth = match extract_session_auth(&state.db, &headers).await {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };

    let user_id: Uuid = match auth.user_id.parse() {
        Ok(u) => u,
        Err(_) => return err_resp::<ResumePositionDto>(StatusCode::BAD_REQUEST, "Invalid user ID".into()).into_response(),
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
    headers: HeaderMap,
) -> Response {
    let auth = match extract_session_auth(&state.db, &headers).await {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };

    let user_id: Uuid = match auth.user_id.parse() {
        Ok(u) => u,
        Err(_) => return err_resp::<Vec<WatchHistoryItemDto>>(StatusCode::BAD_REQUEST, "Invalid user ID".into()).into_response(),
    };
    let movie_id = q.movie_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());
    let episode_id = q.episode_id.as_deref().and_then(|s| s.parse::<Uuid>().ok());
    let limit = q.limit.unwrap_or(20).min(50).max(1);

    match PlaybackRepo::get_watch_history(&state.db, user_id, movie_id, episode_id, limit).await {
        Ok(items) => ok(items).into_response(),
        Err(e) => e.into_response(),
    }
}
