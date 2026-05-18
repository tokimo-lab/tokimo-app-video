use std::sync::Arc;

use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::*;
use serde_json::{Value as JsonValue, json};
use tokimo_package_ffmpeg::{DirectInput, MediaInfo, StreamInfo};
use tokimo_package_iso::{IsoMeta, build_iso_m2ts_input, parse_iso_m2ts};
use tracing::{debug, info};
use uuid::Uuid;

use crate::AppState;
use crate::db::entities::{chapters, subtitles, video_files};
use crate::db::repos::media::file_repo::VideoFileRepo;
use crate::handlers::media::utils::resolve_local_path;
use crate::queue::cancellation::{JobCancel, check_cancel};

type HandlerResult = Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>>;

/// Core ffprobe logic. Called inline from `file_scrape`.
pub async fn run_for_file(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    uuid: Uuid,
    cancel: &JobCancel,
) -> HandlerResult {
    check_cancel(cancel)?;
    let media_file = video_files::Entity::find_by_id(uuid).one(db).await?;
    let Some(media_file) = media_file else {
        return Err(format!("VideoFile not found: {uuid}").into());
    };

    let is_iso = media_file.path.to_ascii_lowercase().ends_with(".iso");
    let (probe, iso_meta) = if is_iso {
        probe_iso(db, state, &media_file).await?
    } else {
        (probe_via_direct(db, state, &media_file).await?, None)
    };

    save_probe_result(db, probe, iso_meta, media_file).await
}

/// Probe a regular (non-ISO) media file via VFS `DirectInput`.
#[allow(unsafe_code)]
async fn probe_via_direct(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    media_file: &video_files::Model,
) -> Result<MediaInfo, Box<dyn std::error::Error + Send + Sync>> {
    let file_id = media_file.id.to_string();
    let target = VideoFileRepo::load_stream_target(db, &file_id)
        .await?
        .ok_or("MediaFileStreamTarget not found")?;

    let source_id = target.source_id.as_deref().ok_or("Media file has no source_id")?;

    let file_size = target.size.ok_or("Media file has unknown size")? as u64;

    let filename_hint = std::path::Path::new(&target.path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string);

    let vfs = state
        .sources
        .ensure_vfs(source_id)
        .await
        .map_err(|e| format!("Failed to get VFS for probe: {e}"))?;

    let ra = vfs.to_read_at(std::path::Path::new(&target.path)).await;
    let direct_input = DirectInput::from_read_at(
        ra,
        file_size,
        filename_hint,
        Some(8 * 1024 * 1024),
    );

    tokio::task::spawn_blocking(move || {
        let result = tokimo_package_ffmpeg::probe_direct(direct_input);
        #[cfg(target_os = "linux")]
        #[allow(unsafe_code)]
        unsafe {
            libc::malloc_trim(0)
        };
        result
    })
    .await?
    .map_err(|e| format!("probe_direct failed: {e}").into())
}

/// Probe an ISO file by probing its inner M2TS stream instead of the raw image bytes.
#[allow(unsafe_code)]
async fn probe_iso(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    media_file: &video_files::Model,
) -> Result<(MediaInfo, Option<IsoMeta>), Box<dyn std::error::Error + Send + Sync>> {
    let file_id = media_file.id.to_string();
    let target = VideoFileRepo::load_stream_target(db, &file_id)
        .await?
        .ok_or("MediaFileStreamTarget not found for ISO")?;

    let source_type = target.source_type.as_deref().unwrap_or("");
    info!(
        "[iso_probe] Probing ISO: {} (source_type={})",
        media_file.path, source_type
    );

    if source_type == "local" {
        let abs_path = resolve_local_path(&target.path, target.source_config.as_ref());
        let bluray_url = format!("bluray:{abs_path}");
        let info = tokio::task::spawn_blocking(move || {
            let result = tokimo_package_ffmpeg::probe_file(&bluray_url);
            #[cfg(target_os = "linux")]
            #[allow(unsafe_code)]
            unsafe {
                libc::malloc_trim(0)
            };
            result
        })
        .await?
        .map_err(|e| format!("bluray probe failed: {e}"))?;
        return Ok((info, None));
    }

    let source_id = target.source_id.as_deref().ok_or("Remote ISO has no source_id")?;
    let file_size = target.size.ok_or("Remote ISO has unknown size")? as u64;

    let vfs = state
        .sources
        .ensure_vfs(source_id)
        .await
        .map_err(|e| format!("Failed to get VFS for ISO probe: {e}"))?;

    let iso_meta = parse_iso_m2ts(&vfs, &target.path, file_size)
        .await
        .map_err(|e| format!("UDF parse failed during probe: {e}"))?;

    let direct_input = build_iso_m2ts_input(vfs.clone(), &target.path, file_size, Some(&iso_meta), None)
        .await
        .map_err(|e| format!("ISO probe failed (libudfread): {e}"))?;

    let info = tokio::task::spawn_blocking(move || {
        let result = tokimo_package_ffmpeg::probe_direct(direct_input);
        #[cfg(target_os = "linux")]
        #[allow(unsafe_code)]
        unsafe {
            libc::malloc_trim(0)
        };
        result
    })
    .await?
    .map_err(|e| format!("probe_direct failed: {e}"))?;

    Ok((info, Some(iso_meta)))
}

async fn save_probe_result(
    db: &DatabaseConnection,
    probe: MediaInfo,
    iso_meta: Option<IsoMeta>,
    media_file: video_files::Model,
) -> HandlerResult {
    let uuid = media_file.id;

    let video_stream = probe
        .streams
        .iter()
        .find(|s| s.codec_type == "video" && s.disposition.get("attached_pic") != Some(&1));

    let video_codec = video_stream.map(|s| s.codec_name.clone());
    let video_width = video_stream.and_then(|s| s.video.as_ref().map(|v| v.width));
    let video_height = video_stream.and_then(|s| s.video.as_ref().map(|v| v.height));
    let video_profile = video_stream.and_then(|s| s.profile.clone());
    let hdr_type = video_stream.and_then(detect_hdr_type);

    let duration_secs = probe.format.duration_secs();
    let duration = if duration_secs > 0.0 {
        Some(duration_secs.round() as i32)
    } else {
        None
    };

    let video_streams_json: Option<JsonValue> = video_stream.map(|s| json!(s));
    let audio_streams: Vec<&StreamInfo> = probe.streams.iter().filter(|s| s.codec_type == "audio").collect();
    let audio_streams_json: Option<JsonValue> = if audio_streams.is_empty() {
        None
    } else {
        Some(json!(audio_streams))
    };

    let subtitle_streams: Vec<&StreamInfo> = probe.streams.iter().filter(|s| s.codec_type == "subtitle").collect();

    let now: DateTimeWithTimeZone = chrono::Utc::now().into();
    let raw_output = json!(probe);

    let mut active: video_files::ActiveModel = media_file.into();
    if let Some(d) = duration {
        active.duration = Set(Some(d));
    }
    if let Some(ref c) = video_codec {
        active.video_codec = Set(Some(c.clone()));
    }
    if let Some(w) = video_width {
        active.video_width = Set(Some(w));
    }
    if let Some(h) = video_height {
        active.video_height = Set(Some(h));
    }
    if let Some(ref p) = video_profile {
        active.video_profile = Set(Some(p.clone()));
    }
    if let Some(ref h) = hdr_type {
        active.hdr_type = Set(Some(h.clone()));
    }
    active.video_streams = Set(video_streams_json);
    active.audio_streams = Set(audio_streams_json);
    active.ffprobe_raw = Set(Some(raw_output));
    active.scanned_at = Set(Some(now));
    active.updated_at = Set(Some(now));
    if let Some(ref meta) = iso_meta {
        let json = serde_json::to_value(meta).unwrap_or(serde_json::Value::Null);
        active.iso_meta = Set(Some(json));
    }

    let txn = db.begin().await?;

    active.update(&txn).await?;

    subtitles::Entity::delete_many()
        .filter(subtitles::Column::FileId.eq(uuid))
        .filter(subtitles::Column::SourceType.eq("embedded"))
        .exec(&txn)
        .await?;

    for stream in &subtitle_streams {
        let lang = stream.tags.get("language").cloned().unwrap_or_else(|| "und".into());
        let title = stream.tags.get("title").cloned();
        let codec = stream.codec_name.clone();
        let is_default = stream.disposition.get("default") == Some(&1);
        let is_forced = stream.disposition.get("forced") == Some(&1);
        let is_hearing_impaired = stream.disposition.get("hearing_impaired") == Some(&1);

        let sub = subtitles::ActiveModel {
            id: Set(Uuid::new_v4()),
            file_id: Set(uuid),
            language: Set(lang),
            title: Set(title),
            source_type: Set("embedded".into()),
            format: Set(codec),
            path: Set(None),
            s3_key: Set(None),
            source: Set(Some("ffprobe".into())),
            source_id: Set(Some(stream.index.to_string())),
            encoding: Set(None),
            is_default: Set(is_default),
            is_forced: Set(is_forced),
            is_hearing_impaired: Set(is_hearing_impaired),
            created_at: Set(now),
        };
        subtitles::Entity::insert(sub).exec(&txn).await?;
    }

    chapters::Entity::delete_many()
        .filter(chapters::Column::FileId.eq(uuid))
        .exec(&txn)
        .await?;

    for ch in &probe.chapters {
        let start_secs: f64 = ch.start_time.parse().unwrap_or(0.0);
        let end_secs: f64 = ch.end_time.parse().unwrap_or(0.0);
        let start_ms = (start_secs * 1000.0).round() as i32;
        let end_ms = (end_secs * 1000.0).round() as i32;

        let chapter = chapters::ActiveModel {
            id: Set(Uuid::new_v4()),
            file_id: Set(uuid),
            index: Set(ch.id as i32),
            title: Set(ch.tags.get("title").cloned()),
            start_time: Set(start_ms),
            thumb_path: Set(None),
            end_time: Set(Some(end_ms)),
        };
        chapters::Entity::insert(chapter).exec(&txn).await?;
    }

    txn.commit().await?;

    debug!(
        media_file_id = %uuid,
        subtitle_count = subtitle_streams.len(),
        chapter_count = probe.chapters.len(),
        "FFI probe completed"
    );

    Ok(Some(json!({
        "mediaFileId": uuid.to_string(),
        "embeddedSubtitleCount": subtitle_streams.len(),
        "chapterCount": probe.chapters.len(),
    })))
}

// ─── HDR detection ───────────────────────────────────────────────────────────

fn detect_hdr_type(video: &StreamInfo) -> Option<String> {
    let dv = detect_dolby_vision(video);
    let hdr_base = detect_hdr_base(video);

    match (dv.as_deref(), hdr_base.as_deref()) {
        (Some(dv_str), Some(base)) => Some(format!("{dv_str}_{base}")),
        (Some(dv_str), None) => Some(dv_str.to_string()),
        (None, Some(base)) => Some(base.to_string()),
        (None, None) => None,
    }
}

fn detect_hdr_base(video: &StreamInfo) -> Option<String> {
    let vi = video.video.as_ref()?;
    let transfer = vi.color_transfer.as_deref().unwrap_or("");
    let primaries = vi.color_primaries.as_deref().unwrap_or("");

    if transfer == "smpte2084" && primaries == "bt2020" {
        if video.has_hdr10_plus {
            Some("hdr10+".into())
        } else {
            Some("hdr10".into())
        }
    } else if transfer == "arib-std-b67" {
        Some("hlg".into())
    } else {
        None
    }
}

fn detect_dolby_vision(video: &StreamInfo) -> Option<String> {
    let side_data = video.side_data_list.as_ref()?;
    let dv_sd = side_data
        .iter()
        .find(|sd| sd.side_data_type.contains("Dolby Vision") || sd.side_data_type.contains("DOVI"))?;

    let profile = dv_sd.dv_profile.unwrap_or(-1);
    let bl_compat = dv_sd.bl_signal_compatibility_id.unwrap_or(-1);
    let el_present = dv_sd.el_present_flag.unwrap_or(0);

    match (profile, bl_compat, el_present) {
        (5 | 10, _, _) | (7 | 8, 1 | 6, _) => Some("dolby_vision".into()),
        (_, _, 1) => Some("dolby_vision_el".into()),
        _ if profile >= 0 => Some("dolby_vision".into()),
        _ => None,
    }
}
