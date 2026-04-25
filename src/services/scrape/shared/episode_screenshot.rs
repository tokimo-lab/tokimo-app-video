//! FFmpeg-based episode still capture.
//!
//! When TMDB has no still image for an episode, we try to capture one locally.
//! Strategy: 10% of duration → 0s (beginning) → give up.
//! When duration is unknown we substitute 10 s for the first attempt.
//!
//! VFS already handles local/remote uniformly via `to_read_at` (local files get
//! a direct syscall fast-path internally), so we don't need to branch on source type.
//!
//! **Memory management:** Each FFmpeg decode context allocates ~100–200 MB through
//! glibc ptmalloc (not Rust/jemalloc). These arenas are not promptly returned to the
//! OS after the context is freed. Two mitigations are applied:
//!   1. `screenshot_semaphore` limits concurrent captures to 4 — capping peak RSS.
//!   2. `malloc_trim(0)` is called on the blocking thread after each capture to
//!      reclaim as much glibc arena memory as possible before the permit is released.

use std::sync::Arc;

use bytes::Bytes;
use ffmpeg_tool::{DirectInput, ImageFormat, VideoScreenshotOptions, capture_video_screenshot_direct};
use tokimo_vfs::Vfs;
use sea_orm::*;
use tracing::{info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::db::entities::{episodes, video_files};
use crate::services::storage::UploadOptions;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Capture and upload an episode still if the episode currently has none.
/// All errors are logged as warnings — a missing still is non-fatal.
pub async fn maybe_capture_episode_screenshot(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    episode_id: Uuid,
    media_file_id: Uuid,
    vfs: Arc<Vfs>,
    file_path: &str,
) {
    if let Err(e) = do_capture(db, state, episode_id, media_file_id, vfs, file_path).await {
        warn!("[episode_screenshot] Failed for episode {episode_id}: {e}");
    }
}

#[allow(unsafe_code)]
async fn do_capture(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    episode_id: Uuid,
    media_file_id: Uuid,
    vfs: Arc<Vfs>,
    file_path: &str,
) -> Result<(), BoxError> {
    // Skip when the episode already has a still image.
    let episode = episodes::Entity::find_by_id(episode_id).one(db).await?;
    let Some(episode) = episode else {
        return Ok(());
    };
    if episode.still_path.is_some() {
        return Ok(());
    }

    // Read duration + file size set by FFprobe / media_file creation.
    let vf = video_files::Entity::find_by_id(media_file_id).one(db).await?;
    let (duration_secs, file_size) = match vf {
        Some(ref v) => (v.duration.map(f64::from), v.size.unwrap_or(0) as u64),
        None => (None, 0),
    };

    // Primary position: 10% of duration, or 10 s when duration is unknown.
    // Attempt order: primary → 0s (beginning) → give up.
    let primary_pos = duration_secs.map_or(10.0, |d| d * 0.1);
    let positions: &[f64] = if primary_pos < 0.5 { &[0.0] } else { &[primary_pos, 0.0] };

    let filename_hint = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string);

    let opts_base = VideoScreenshotOptions {
        width: Some(1280),
        format: ImageFormat::Jpeg,
        quality: 2,
        prefer_hardware: true,
        ..Default::default()
    };

    let mut screenshot_bytes: Option<Vec<u8>> = None;
    for &pos in positions {
        // `vfs.to_read_at` already picks the local syscall fast-path for local
        // sources, so no source_type branch is needed here.
        let ra = vfs.to_read_at(std::path::Path::new(file_path)).await;
        let direct = DirectInput::from_read_at(ra, file_size, filename_hint.clone(), None);
        let opts = VideoScreenshotOptions {
            offset_secs: pos,
            ..opts_base.clone()
        };

        // Acquire permit BEFORE entering the blocking thread so we bound how many
        // concurrent FFmpeg decode contexts are alive at once.  The permit is held
        // until the blocking thread finishes (including malloc_trim).
        let _permit = state
            .screenshot_semaphore
            .acquire()
            .await
            .map_err(|e| format!("screenshot semaphore closed: {e}"))?;

        match tokio::task::spawn_blocking(move || {
            let result = capture_video_screenshot_direct(direct, &opts);
            // Return glibc arenas used by FFmpeg to the OS.  This call is cheap
            // (~1 ms) and reclaims hundreds of MB that ptmalloc would otherwise
            // hold in per-thread arenas indefinitely.
            #[cfg(target_os = "linux")]
            #[allow(unsafe_code)]
            unsafe {
                libc::malloc_trim(0)
            };
            result
        })
        .await
        {
            Ok(Ok(bytes)) => {
                screenshot_bytes = Some(bytes);
                break;
            }
            Ok(Err(e)) => warn!("[episode_screenshot] screenshot at {pos:.1}s failed: {e}"),
            Err(e) => warn!("[episode_screenshot] spawn_blocking panic at {pos:.1}s: {e}"),
        }
    }

    let Some(bytes) = screenshot_bytes else {
        info!("[episode_screenshot] All attempts exhausted for episode {episode_id}, skipping");
        return Ok(());
    };

    // Upload to S3.
    let storage_key = format!("library-images/episodes/{episode_id}/still.jpg");
    state
        .storage
        .upload(
            &storage_key,
            Bytes::from(bytes),
            Some(UploadOptions {
                content_type: Some("image/jpeg".to_string()),
            }),
        )
        .await
        .map_err(|e| format!("storage upload failed: {e}"))?;

    // Persist still_path on the episode.
    let mut active: episodes::ActiveModel = episode.into();
    active.still_path = Set(Some(format!("/storage/{storage_key}")));
    active.update(db).await?;

    info!("[episode_screenshot] Captured still for episode {episode_id}");
    Ok(())
}
