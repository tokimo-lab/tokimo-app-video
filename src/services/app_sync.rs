use crate::db::ApiDateTimeExt;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use next_fs::Vfs;
use regex_lite::Regex;
use sea_orm::*;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use rust_client_api::media_servers::{
    emby::EmbyClient, jellyfin::JellyfinClient, plex::PlexClient, traits::MediaItem,
};

use crate::db::entities::{
    episodes, file_systems, app_file_systems, media_credits, media_files, apps, media_servers,
    movies, music_albums, music_tracks, persons, photos, seasons, tv_shows,
};
use crate::db::repos::job_repo::JobRepo;
use crate::db::repos::media::AppRepo;
use crate::error::AppError;
use crate::handlers::media::fs::{walk_video_files_streaming, walk_files_streaming, AUDIO_EXTENSIONS, NOVEL_EXTENSIONS, PHOTO_EXTENSIONS};
use crate::services::media::source::SourceRegistry;

/// Types of media libraries (matches TS AppType).
fn is_movie_type(lib_type: &str) -> bool {
    matches!(lib_type, "movie" | "adult" | "custom" | "online_video")
}

fn is_tv_type(lib_type: &str) -> bool {
    matches!(lib_type, "tv" | "anime")
}

fn is_music_type(lib_type: &str) -> bool {
    lib_type == "music"
}

fn is_novel_type(lib_type: &str) -> bool {
    lib_type == "novel"
}

fn is_photo_type(lib_type: &str) -> bool {
    lib_type == "photo"
}

/// Remote file system source types (network protocols + cloud drives).
fn is_remote_fs_type(source_type: &str) -> bool {
    matches!(
        source_type,
        "smb" | "nfs" | "webdav" | "ftp" | "sftp" | "s3"
            | "115cloud"
            | "aliyundrive"
            | "baidu_netdisk"
            | "quark"
    )
}

/// Convert an absolute `root_path` from `app_file_systems` to a VFS-relative path.
///
/// For local sources the DB may store the full filesystem path
/// (e.g. `/home/william/media/movie`) while the local driver's root is already
/// `/home/william/media`. The VFS expects a path relative to the driver root
/// (e.g. `/movie`), so we strip the driver root prefix.
fn to_vfs_path(root_path: &str, source: &file_systems::Model) -> String {
    if source.r#type != "local" {
        return root_path.to_string();
    }
    let driver_root = source
        .config
        .as_ref()
        .and_then(|c| {
            c.get("root")
                .or_else(|| c.get("root_folder_path"))
                .or_else(|| c.get("path"))
        })
        .and_then(|v| v.as_str());
    let Some(driver_root) = driver_root else {
        return root_path.to_string();
    };
    let driver_root = driver_root.trim_end_matches('/');
    if root_path.starts_with(driver_root) && root_path.len() > driver_root.len() {
        let rel = &root_path[driver_root.len()..];
        if rel.starts_with('/') {
            return rel.to_string();
        }
    }
    if root_path == driver_root {
        return "/".to_string();
    }
    root_path.to_string()
}

const SERVER_SYNC_PAGE_SIZE: u32 = 100;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusOutput {
    pub app_id: String,
    pub status: String,
    pub last_sync_at: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub total_jobs: u64,
}

// ── music sync types ────────────────────────────────────────────────────

/// Audio tag info extracted from a file via lofty.
struct AudioTagInfo {
    title: Option<String>,
    artist: Option<String>,
    album_artist: Option<String>,
    album: Option<String>,
    track_number: Option<i32>,
    disc_number: Option<i32>,
    year: Option<i32>,
    genre: Option<String>,
    duration: Option<i32>,
    bitrate: Option<i32>,
    sample_rate: Option<i32>,
    codec: Option<String>,
    mb_track_id: Option<String>,
    mb_album_id: Option<String>,
}

/// Collected audio file info for music sync.
struct CollectedAudioFile {
    file_path: String,
    dir_path: String,
    file_size: u64,
    mtime: i64,
    source_id: Uuid,
    tags: Option<AudioTagInfo>,
}

/// Grouped album info.
struct AlbumGroup {
    artist_name: String,
    album_title: String,
    year: Option<i32>,
    dir_path: String,
    files: Vec<CollectedAudioFile>,
}

pub struct AppSyncService;

impl AppSyncService {
    /// Get sync status for a single library.
    pub async fn get_sync_status(
        db: &DatabaseConnection,
        app_id: Uuid,
    ) -> Result<SyncStatusOutput, AppError> {
        let (status, last_sync_at) = AppRepo::get_sync_status(db, app_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("app {app_id} not found")))?;

        Ok(SyncStatusOutput {
            app_id: app_id.to_string(),
            status,
            last_sync_at: last_sync_at.to_api_datetime(),
        })
    }

    /// Get sync statuses for all libraries.
    pub async fn get_all_sync_statuses(
        db: &DatabaseConnection,
    ) -> Result<Vec<SyncStatusOutput>, AppError> {
        let libraries = AppRepo::list_all(db).await?;
        Ok(libraries
            .into_iter()
            .map(|lib| SyncStatusOutput {
                app_id: lib.id.to_string(),
                status: lib.sync_status,
                last_sync_at: lib.last_sync_at.to_api_datetime(),
            })
            .collect())
    }

    /// Execute full app sync.
    ///
    /// Walks file-system sources, queries media servers, and writes pending
    /// `file_scrape` / `media_server_item_sync` job records.  A separate TS
    /// worker polls the `jobs` table and dispatches into BullMQ.
    pub async fn execute_sync(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        app_id: Uuid,
        clear_data: bool,
        http_client: reqwest::Client,
    ) -> Result<SyncResult, AppError> {
        // 1. Fetch library
        let library = AppRepo::get_by_id(db, app_id)
            .await?
            .ok_or_else(|| AppError::NotFound("app not found".into()))?;

        let lib_type = &library.r#type;
        let is_movie = is_movie_type(lib_type);
        let is_tv = is_tv_type(lib_type);
        let is_music = is_music_type(lib_type);

        info!(
            "Starting sync for library \"{}\" (id={}, type={})",
            library.name, app_id, lib_type
        );

        // 2. Guard against concurrent syncs for the same library
        if library.sync_status == "syncing" && !clear_data {
            warn!(
                "Library \"{}\" is already syncing, skipping duplicate sync request",
                library.name
            );
            return Err(AppError::Conflict(
                "Library is already syncing".into(),
            ));
        }

        // 3. Update sync status to "syncing"
        AppRepo::update_sync_status(db, app_id, "syncing", None).await?;

        // Wrap the actual work so we can catch errors and set status to "failed".
        let result = Self::do_sync(
            db, sources, &library, lib_type, is_movie, is_tv, is_music, clear_data, http_client,
        )
        .await;

        match &result {
            Ok(sync_result) => {
                let now = Utc::now().fixed_offset();
                AppRepo::update_sync_status(db, app_id, "completed", Some(now))
                    .await?;
                info!(
                    "Sync completed: \"{}\" — {} jobs dispatched",
                    library.name, sync_result.total_jobs
                );

                // Auto-enqueue AI processing jobs for photo libraries
                if is_photo_type(lib_type) && sync_result.total_jobs > 0 {
                    Self::enqueue_photo_ai_jobs(db, app_id).await;
                }
            }
            Err(err) => {
                error!("Sync failed for library \"{}\": {}", library.name, err);
                let _ = AppRepo::update_sync_status(db, app_id, "failed", None).await;
            }
        }

        result
    }

    /// Enqueue batch AI processing jobs (face detect, OCR, CLIP, reverse geocode)
    /// for a photo library. Skips job types that already have a pending job.
    /// Respects per-app settings: `autoOcr`, `autoClip`, `autoFace`, `autoGeo`
    /// (all default to `true` when absent).
    pub async fn enqueue_photo_ai_jobs(db: &DatabaseConnection, app_id: Uuid) {
        // Read per-app AI flags from app.settings JSONB
        let app_settings = match apps::Entity::find_by_id(app_id).one(db).await {
            Ok(Some(app)) => app.settings.unwrap_or_else(|| json!({})),
            Ok(None) => {
                warn!("[auto_ai] App {app_id} not found, skipping AI jobs");
                return;
            }
            Err(e) => {
                warn!("[auto_ai] Failed to load app {app_id}: {e}");
                return;
            }
        };

        let auto_flag = |key: &str| -> bool {
            app_settings.get(key).and_then(|v| v.as_bool()).unwrap_or(true)
        };

        let ai_job_types: Vec<&str> = [
            ("photo_face_detect", auto_flag("autoFace")),
            ("photo_ocr", auto_flag("autoOcr")),
            ("photo_clip", auto_flag("autoClip")),
            ("photo_reverse_geocode", auto_flag("autoGeo")),
        ]
        .into_iter()
        .filter(|(job_type, enabled)| {
            if !enabled {
                info!("[auto_ai] Skipping {job_type}: disabled in app settings");
            }
            *enabled
        })
        .map(|(job_type, _)| job_type)
        .collect();

        let payload = json!({ "appId": app_id.to_string() });

        for job_type in ai_job_types {
            match JobRepo::count_pending(db, job_type).await {
                Ok(n) if n > 0 => {
                    info!("[auto_ai] Skipping {job_type}: {n} pending job(s) already exist");
                }
                Ok(_) => {
                    match JobRepo::create_job(db, job_type, payload.clone(), None).await {
                        Ok(_) => info!("[auto_ai] Enqueued {job_type} for app {app_id}"),
                        Err(e) => warn!("[auto_ai] Failed to enqueue {job_type}: {e}"),
                    }
                }
                Err(e) => {
                    warn!("[auto_ai] Failed to check pending {job_type}: {e}");
                }
            }
        }
    }

    // ── core sync logic ─────────────────────────────────────────────────

    async fn do_sync(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        library: &apps::Model,
        lib_type: &str,
        is_movie: bool,
        is_tv: bool,
        is_music: bool,
        clear_data: bool,
        http_client: reqwest::Client,
    ) -> Result<SyncResult, AppError> {
        let app_id = library.id;

        // 3. Optional data clear
        if clear_data {
            Self::clear_library_data(db, app_id, lib_type).await?;
        }

        let last_sync_at = if !clear_data {
            library.last_sync_at
        } else {
            None
        };
        let mut total_jobs = 0u64;

        // 4. Process file system sources
        let fs_sources = AppRepo::get_sources(db, app_id).await?;
        for link in &fs_sources {
            let source = file_systems::Entity::find_by_id(link.source_id)
                .one(db)
                .await?
                .ok_or_else(|| {
                    AppError::NotFound(format!("source {} not found", link.source_id))
                })?;

            let jobs = Self::sync_fs_source(
                db,
                sources,
                app_id,
                lib_type,
                is_movie,
                is_tv,
                is_music,
                &source,
                &link.root_path,
            )
            .await?;
            total_jobs += jobs;
        }

        // 5. Process media server links
        let server_links = AppRepo::get_server_links(db, app_id).await?;
        for link in &server_links {
            let server = media_servers::Entity::find_by_id(link.server_id)
                .one(db)
                .await?
                .ok_or_else(|| {
                    AppError::NotFound(format!("server {} not found", link.server_id))
                })?;

            let jobs = Self::sync_media_server(
                db,
                http_client.clone(),
                &server,
                &link.server_app_id,
                app_id,
                lib_type,
                is_movie,
                is_tv,
                last_sync_at,
            )
            .await?;
            total_jobs += jobs;
        }

        Ok(SyncResult { total_jobs })
    }

    // ── clear library data ──────────────────────────────────────────────

    async fn clear_library_data(
        db: &DatabaseConnection,
        app_id: Uuid,
        lib_type: &str,
    ) -> Result<(), AppError> {
        info!("Clearing data for library {app_id} (type={lib_type})");

        // Cancel all pending/running jobs for this library
        let cancelled = JobRepo::cancel_jobs_by_app_id(db, app_id).await?;
        if cancelled > 0 {
            info!("  Cancelled {cancelled} pending/running jobs");
        }

        // Collect source_ids for this library (used to clean orphaned media_files)
        let source_ids: Vec<Uuid> = app_file_systems::Entity::find()
            .filter(app_file_systems::Column::AppId.eq(app_id))
            .all(db)
            .await?
            .into_iter()
            .map(|lfs| lfs.source_id)
            .collect();

        if is_movie_type(lib_type) {
            // Delete media_files linked to movies in this library BEFORE deleting
            // movies (otherwise FK SET NULL leaves orphan rows that block re-scrape).
            let movie_ids: Vec<Uuid> = movies::Entity::find()
                .filter(movies::Column::AppId.eq(app_id))
                .all(db)
                .await?
                .into_iter()
                .map(|m| m.id)
                .collect();
            if !movie_ids.is_empty() {
                let mf_deleted = media_files::Entity::delete_many()
                    .filter(media_files::Column::MovieId.is_in(movie_ids.clone()))
                    .exec(db)
                    .await?
                    .rows_affected;
                info!("  Deleted {mf_deleted} media files (linked to movies)");
            }

            let deleted = movies::Entity::delete_many()
                .filter(movies::Column::AppId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} movies");
        } else if is_tv_type(lib_type) {
            // Delete media_files linked to episodes of tv shows in this library.
            let show_ids: Vec<Uuid> = tv_shows::Entity::find()
                .filter(tv_shows::Column::AppId.eq(app_id))
                .all(db)
                .await?
                .into_iter()
                .map(|s| s.id)
                .collect();
            if !show_ids.is_empty() {
                let episode_ids: Vec<Uuid> = episodes::Entity::find()
                    .filter(episodes::Column::TvShowId.is_in(show_ids))
                    .all(db)
                    .await?
                    .into_iter()
                    .map(|e| e.id)
                    .collect();
                if !episode_ids.is_empty() {
                    let mf_deleted = media_files::Entity::delete_many()
                        .filter(media_files::Column::EpisodeId.is_in(episode_ids))
                        .exec(db)
                        .await?
                        .rows_affected;
                    info!("  Deleted {mf_deleted} media files (linked to episodes)");
                }
            }

            let deleted = tv_shows::Entity::delete_many()
                .filter(tv_shows::Column::AppId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} tv shows (cascade: seasons + episodes)");
        } else if is_music_type(lib_type) {
            let deleted = music_albums::Entity::delete_many()
                .filter(music_albums::Column::AppId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} music albums");
        } else if is_novel_type(lib_type) {
            use crate::db::entities::{novel_chapters, novel_volumes, novels};

            let novel_ids: Vec<Uuid> = novels::Entity::find()
                .filter(novels::Column::AppId.eq(app_id))
                .all(db)
                .await?
                .into_iter()
                .map(|n| n.id)
                .collect();
            if !novel_ids.is_empty() {
                let ch_deleted = novel_chapters::Entity::delete_many()
                    .filter(novel_chapters::Column::NovelId.is_in(novel_ids.clone()))
                    .exec(db)
                    .await?
                    .rows_affected;
                info!("  Deleted {ch_deleted} novel chapters");

                let vol_deleted = novel_volumes::Entity::delete_many()
                    .filter(novel_volumes::Column::NovelId.is_in(novel_ids.clone()))
                    .exec(db)
                    .await?
                    .rows_affected;
                info!("  Deleted {vol_deleted} novel volumes");

                let mf_deleted = media_files::Entity::delete_many()
                    .filter(media_files::Column::NovelId.is_in(novel_ids.clone()))
                    .exec(db)
                    .await?
                    .rows_affected;
                info!("  Deleted {mf_deleted} media files (linked to novels)");
            }

            let deleted = novels::Entity::delete_many()
                .filter(novels::Column::AppId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} novels");
        } else if is_photo_type(lib_type) {
            let deleted = photos::Entity::delete_many()
                .filter(photos::Column::AppId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} photos");
        }

        // Delete orphaned media_files (unlinked, with movie_id/episode_id/track_id all NULL)
        // that belong to this library's sources.
        if !source_ids.is_empty() {
            let orphan_deleted = media_files::Entity::delete_many()
                .filter(media_files::Column::SourceId.is_in(source_ids))
                .filter(media_files::Column::MovieId.is_null())
                .filter(media_files::Column::EpisodeId.is_null())
                .filter(media_files::Column::TrackId.is_null())
                .exec(db)
                .await?
                .rows_affected;
            if orphan_deleted > 0 {
                info!("  Deleted {orphan_deleted} orphaned media files");
            }
        }

        // Reset last_sync_at
        AppRepo::update_sync_status(db, app_id, "syncing", None).await?;
        Ok(())
    }

    // ── file system source sync ─────────────────────────────────────────

    /// Batch size for flushing accumulated jobs to DB.
    const JOB_BATCH_FLUSH_SIZE: usize = 50;

    async fn sync_fs_source(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        app_id: Uuid,
        lib_type: &str,
        is_movie: bool,
        is_tv: bool,
        is_music: bool,
        source: &file_systems::Model,
        root_path: &str,
    ) -> Result<u64, AppError> {
        let source_type = &source.r#type;

        if is_novel_type(lib_type) {
            info!(
                "Novel app sync: walking file system source \"{}\" for novel files",
                source.name
            );
        }

        if is_music {
            return Self::sync_music_source(db, sources, app_id, source, root_path).await;
        }

        let is_local = source_type == "local";
        let is_remote = is_remote_fs_type(source_type);

        if !is_local && !is_remote {
            warn!(
                "Unsupported source type \"{}\" for source \"{}\", skipping",
                source_type, source.name
            );
            return Ok(0);
        }

        // Get VFS handle
        let source_id_str = source.id.to_string();
        let vfs = sources.ensure_vfs(&source_id_str).await.map_err(|e| {
            AppError::Internal(format!(
                "Failed to get VFS for source {} ({}): {}",
                source.name, source_id_str, e
            ))
        })?;

        // Convert absolute root_path to VFS-relative path
        let vfs_root = to_vfs_path(root_path, source);

        // Spawn concurrent walk as a background task, streaming results through channel
        let (tx, mut rx) = mpsc::channel::<crate::handlers::media::fs::VideoFileInfo>(256);
        let walk_root = vfs_root.clone();
        let walk_source_id = source_id_str.clone();
        let is_photo = is_photo_type(lib_type);
        let is_novel = is_novel_type(lib_type);
        let walk_handle = tokio::spawn(async move {
            if is_photo {
                walk_files_streaming(vfs, &walk_root, &walk_source_id, &PHOTO_EXTENSIONS, tx).await
            } else if is_novel {
                walk_files_streaming(vfs, &walk_root, &walk_source_id, &NOVEL_EXTENSIONS, tx).await
            } else {
                walk_video_files_streaming(vfs, &walk_root, &walk_source_id, tx).await
            }
        });

        // Consume files as they arrive — check DB + accumulate jobs incrementally
        let source_id = source.id;
        let mut seen_paths = HashSet::new();
        let mut jobs_batch: Vec<(&str, serde_json::Value, Option<serde_json::Value>)> = Vec::new();
        let mut total_jobs = 0u64;
        let mut skipped = 0u64;

        // For novels: buffer .txt files grouped by directory, emit one job per directory.
        // Non-txt novel files (epub/mobi/etc.) get individual jobs like before.
        let mut novel_dir_files: HashMap<String, Vec<crate::handlers::media::fs::VideoFileInfo>> =
            HashMap::new();

        while let Some(video) = rx.recv().await {
            seen_paths.insert(video.file_path.clone());
            let checksum = format!("{}:{}", video.file_size, video.mtime);

            // Photo and novel libraries skip the media_files dedup — the handlers
            // check their own tables directly for idempotency.
            if !is_photo && !is_novel {
                let existing =
                    Self::find_existing_media_file(db, source_id, &video.file_path, is_movie, is_tv)
                        .await?;

                if let Some(existing) = existing {
                    let checksum_matches = existing.checksum.as_deref() == Some(&checksum);

                    if checksum_matches
                        && !Self::needs_artwork_backfill(db, &existing, is_movie, is_tv).await?
                    {
                        skipped += 1;
                        continue; // Unchanged and has artwork — skip
                    }

                    if !checksum_matches {
                        Self::reset_media_file_link(db, existing.id, &checksum).await?;
                    }
                }
            }

            // Novel .txt files: group by parent directory for chapter-based novels
            if is_novel && video.file_path.to_lowercase().ends_with(".txt") {
                novel_dir_files
                    .entry(video.dir_path.clone())
                    .or_default()
                    .push(video);
                continue;
            }

            let job_type = if is_novel { "novel_scrape" } else { "file_scrape" };
            jobs_batch.push((
                job_type,
                json!({
                    "filePath": video.file_path,
                    "dirPath": video.dir_path,
                    "fileSize": video.file_size,
                    "checksum": checksum,
                    "appId": app_id.to_string(),
                    "sourceId": source_id.to_string(),
                    "libType": lib_type,
                }),
                None,
            ));

            // Flush batch periodically
            if jobs_batch.len() >= Self::JOB_BATCH_FLUSH_SIZE {
                total_jobs +=
                    JobRepo::create_jobs_batch(db, std::mem::take(&mut jobs_batch)).await?;
            }
        }

        // Emit consolidated novel directory jobs (one per directory of .txt chapters)
        for (dir_path, files) in &novel_dir_files {
            let chapter_files: Vec<serde_json::Value> = files
                .iter()
                .map(|f| {
                    json!({
                        "filePath": f.file_path,
                        "fileSize": f.file_size,
                        "checksum": format!("{}:{}", f.file_size, f.mtime),
                    })
                })
                .collect();
            let total_size: u64 = files.iter().map(|f| f.file_size).sum();

            jobs_batch.push((
                "novel_scrape",
                json!({
                    "dirPath": dir_path,
                    "chapterFiles": chapter_files,
                    "totalSize": total_size,
                    "appId": app_id.to_string(),
                    "sourceId": source_id.to_string(),
                    "libType": lib_type,
                }),
                None,
            ));

            if jobs_batch.len() >= Self::JOB_BATCH_FLUSH_SIZE {
                total_jobs +=
                    JobRepo::create_jobs_batch(db, std::mem::take(&mut jobs_batch)).await?;
            }
        }

        // Flush remaining jobs
        if !jobs_batch.is_empty() {
            total_jobs += JobRepo::create_jobs_batch(db, jobs_batch).await?;
        }

        // Wait for walk to complete and check for errors
        let walk_stats = walk_handle
            .await
            .map_err(|e| {
                AppError::Internal(format!(
                    "Walk task panicked for source \"{}\": {}",
                    source.name, e
                ))
            })?
            .map_err(|e| {
                AppError::Internal(format!(
                    "Failed to walk source \"{}\" root={}: {}",
                    source.name, vfs_root, e
                ))
            })?;

        info!(
            "[{}({})] Walk done: {} dirs, {} videos found, {} unchanged (skipped), {} jobs queued under \"{}\"",
            source.name, source_type, walk_stats.visited_dirs, walk_stats.found_videos,
            skipped, total_jobs, vfs_root
        );

        // Cleanup missing files (use vfs_root so DB paths match walk output)
        if is_photo {
            Self::cleanup_missing_photos(db, app_id, source_id, &vfs_root, &seen_paths)
                .await?;
        } else {
            Self::cleanup_missing_files(
                db,
                app_id,
                source_id,
                source_type,
                &vfs_root,
                &seen_paths,
            )
            .await?;
        }

        Ok(total_jobs)
    }

    // ── music sync ──────────────────────────────────────────────────────

    /// Audio MIME types by extension.
    fn audio_mime_type(file_path: &str) -> &'static str {
        let ext = file_path
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "flac" => "audio/flac",
            "mp3" => "audio/mpeg",
            "m4a" | "alac" => "audio/mp4",
            "ogg" => "audio/ogg",
            "opus" => "audio/opus",
            "wav" => "audio/wav",
            "aac" => "audio/aac",
            "wma" => "audio/x-ms-wma",
            "ape" => "audio/x-ape",
            "dsf" => "audio/dsf",
            "dff" => "audio/dff",
            "aiff" | "aif" => "audio/aiff",
            _ => "audio/unknown",
        }
    }

    /// Cover art filenames to search for in an album directory.
    const COVER_ART_NAMES: &'static [&'static str] = &[
        "cover.jpg",
        "cover.png",
        "folder.jpg",
        "folder.png",
        "front.jpg",
        "front.png",
        "album.jpg",
        "album.png",
    ];

    /// Read audio tags from a local file using lofty.
    fn read_audio_tags(path: &std::path::Path) -> Option<AudioTagInfo> {
        use lofty::file::{AudioFile, TaggedFileExt};
        use lofty::tag::Accessor;

        let tagged_file = lofty::read_from_path(path).ok()?;
        let properties = tagged_file.properties();
        let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());

        let (title, artist, album_artist, album, track_number, disc_number, year, genre, mb_track_id, mb_album_id) =
            if let Some(tag) = tag {
                (
                    tag.title().map(|s| s.to_string()),
                    tag.artist().map(|s| s.to_string()),
                    tag.get_string(&lofty::tag::ItemKey::AlbumArtist).map(|s| s.to_string()),
                    tag.album().map(|s| s.to_string()),
                    tag.track().map(|n| n as i32),
                    tag.disk().map(|n| n as i32),
                    tag.year().map(|n| n as i32),
                    tag.genre().map(|s| s.to_string()),
                    tag.get_string(&lofty::tag::ItemKey::MusicBrainzRecordingId).map(|s| s.to_string()),
                    tag.get_string(&lofty::tag::ItemKey::MusicBrainzReleaseId).map(|s| s.to_string()),
                )
            } else {
                (None, None, None, None, None, None, None, None, None, None)
            };

        let duration_secs = if properties.duration().as_secs() > 0 {
            Some(properties.duration().as_secs() as i32)
        } else {
            None
        };

        let bitrate = properties.audio_bitrate().map(|b| b as i32);
        let sample_rate = properties.sample_rate().map(|r| r as i32);

        let codec = {
            let file_type = tagged_file.file_type();
            Some(format!("{:?}", file_type))
        };

        Some(AudioTagInfo {
            title,
            artist,
            album_artist,
            album,
            track_number,
            disc_number,
            year,
            genre,
            duration: duration_secs,
            bitrate,
            sample_rate,
            codec,
            mb_track_id,
            mb_album_id,
        })
    }

    /// Parse music filename to extract track number, title, and artist.
    /// Patterns: "01. Artist - Title", "01 - Title", "01 Title", fallback to filename.
    fn parse_music_filename(file_name: &str, parent_dir: Option<&str>) -> (Option<i32>, Option<String>, Option<String>, Option<String>) {
        let dot_pos = file_name.rfind('.');
        let name = if let Some(pos) = dot_pos {
            &file_name[..pos]
        } else {
            file_name
        };

        let mut track_number: Option<i32> = None;
        let mut artist: Option<String> = None;
        let mut track_title: Option<String> = None;

        // Pattern 1: "01. Artist - Title" or "01 - Artist - Title"
        let re1 = Regex::new(r"^(\d{1,3})[.\s]+(.+?)\s*-\s*(.+)$").unwrap();
        if let Some(caps) = re1.captures(name) {
            track_number = caps.get(1).and_then(|m| m.as_str().parse().ok());
            artist = caps.get(2).map(|m| m.as_str().trim().to_string());
            track_title = caps.get(3).map(|m| m.as_str().trim().to_string());
        }

        // Pattern 2: "01 - Title" (no artist)
        if track_title.is_none() {
            let re2 = Regex::new(r"^(\d{1,3})\s*[-–.]\s*(.+)$").unwrap();
            if let Some(caps) = re2.captures(name) {
                track_number = caps.get(1).and_then(|m| m.as_str().parse().ok());
                track_title = caps.get(2).map(|m| m.as_str().trim().to_string());
            }
        }

        // Pattern 3: "01 Title" (number then space)
        if track_title.is_none() {
            let re3 = Regex::new(r"^(\d{1,3})\s+(.+)$").unwrap();
            if let Some(caps) = re3.captures(name) {
                track_number = caps.get(1).and_then(|m| m.as_str().parse().ok());
                track_title = caps.get(2).map(|m| m.as_str().trim().to_string());
            }
        }

        // Fallback: entire filename as title
        if track_title.is_none() {
            track_title = Some(name.to_string());
        }

        // Album from parent directory (strip trailing year like "(2024)" or "[2024]")
        let album = parent_dir.map(|d| {
            let re_year = Regex::new(r"\s*[(\[][0-9]{4}[)\]]\s*$").unwrap();
            re_year.replace(d, "").trim().to_string()
        });

        (track_number, track_title, artist, album)
    }

    /// Get album info from a collected file — prefer tags, fall back to filename parsing.
    fn get_album_info(file: &CollectedAudioFile) -> (String, String, Option<i32>) {
        if let Some(ref tags) = file.tags {
            if let Some(ref album) = tags.album {
                let artist_name = tags
                    .album_artist
                    .clone()
                    .or_else(|| tags.artist.clone())
                    .unwrap_or_else(|| "Unknown Artist".to_string());
                return (artist_name, album.clone(), tags.year);
            }
        }

        let file_name = file
            .file_path
            .rsplit('/')
            .next()
            .unwrap_or(&file.file_path);
        let parent_dir = file.dir_path.rsplit('/').next();

        let (_, _, parsed_artist, _) =
            Self::parse_music_filename(file_name, parent_dir);

        let artist_name = parsed_artist
            .or_else(|| file.tags.as_ref().and_then(|t| t.artist.clone()))
            .unwrap_or_else(|| "Unknown Artist".to_string());

        let dir_name = file
            .dir_path
            .rsplit('/')
            .next()
            .unwrap_or("Unknown Album");
        let album_title = dir_name.to_string();

        let year = file.tags.as_ref().and_then(|t| t.year);
        (artist_name, album_title, year)
    }

    /// Group collected audio files into album groups.
    fn group_files_into_albums(files: Vec<CollectedAudioFile>) -> Vec<AlbumGroup> {
        let mut groups: HashMap<String, AlbumGroup> = HashMap::new();
        for file in files {
            let (artist_name, album_title, year) = Self::get_album_info(&file);
            let key = format!(
                "{}||{}",
                artist_name.to_lowercase(),
                album_title.to_lowercase()
            );
            let group = groups.entry(key).or_insert_with(|| AlbumGroup {
                artist_name: artist_name.clone(),
                album_title: album_title.clone(),
                year,
                dir_path: file.dir_path.clone(),
                files: Vec::new(),
            });
            if group.year.is_none() && year.is_some() {
                group.year = year;
            }
            group.files.push(file);
        }
        groups.into_values().collect()
    }

    /// Find or create a Person record by name.
    async fn find_or_create_person(
        db: &DatabaseConnection,
        name: &str,
    ) -> Result<Uuid, AppError> {
        let existing = persons::Entity::find()
            .filter(persons::Column::Name.eq(name))
            .one(db)
            .await?;
        if let Some(p) = existing {
            return Ok(p.id);
        }

        let id = Uuid::new_v4();
        let active = persons::ActiveModel {
            id: Set(id),
            name: Set(name.to_string()),
            ..Default::default()
        };
        persons::Entity::insert(active).exec(db).await?;
        Ok(id)
    }

    /// Find or create a MusicAlbum for the given group.
    async fn find_or_create_album(
        db: &DatabaseConnection,
        app_id: Uuid,
        group: &AlbumGroup,
    ) -> Result<Uuid, AppError> {
        // Find existing albums with matching title in this library
        let candidates = music_albums::Entity::find()
            .filter(music_albums::Column::AppId.eq(app_id))
            .filter(music_albums::Column::Title.eq(&group.album_title))
            .find_with_related(media_credits::Entity)
            .all(db)
            .await?;

        // Match by artist name via credits → person
        for (album, credits) in &candidates {
            for credit in credits {
                if let Some(person) = persons::Entity::find_by_id(credit.person_id)
                    .one(db)
                    .await?
                {
                    if person.name.to_lowercase() == group.artist_name.to_lowercase() {
                        return Ok(album.id);
                    }
                }
            }
        }

        let max_disc = group
            .files
            .iter()
            .filter_map(|f| f.tags.as_ref().and_then(|t| t.disc_number))
            .max()
            .unwrap_or(1);

        let sort_title = {
            let re = Regex::new(r"(?i)^(the|a|an)\s+").unwrap();
            re.replace(&group.album_title, "").to_string()
        };

        let id = Uuid::new_v4();
        let now = Utc::now().fixed_offset();
        let active = music_albums::ActiveModel {
            id: Set(id),
            app_id: Set(app_id),
            title: Set(group.album_title.clone()),
            sort_title: Set(Some(sort_title)),
            year: Set(group.year),
            total_tracks: Set(Some(group.files.len() as i32)),
            total_discs: Set(Some(max_disc)),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        music_albums::Entity::insert(active).exec(db).await?;
        Ok(id)
    }

    /// Ensure an "artist" MediaCredit exists linking person to album.
    async fn ensure_artist_credit(
        db: &DatabaseConnection,
        album_id: Uuid,
        person_id: Uuid,
    ) -> Result<(), AppError> {
        let existing = media_credits::Entity::find()
            .filter(media_credits::Column::PersonId.eq(person_id))
            .filter(media_credits::Column::AlbumId.eq(album_id))
            .filter(media_credits::Column::Role.eq("artist"))
            .one(db)
            .await?;
        if existing.is_some() {
            return Ok(());
        }

        let active = media_credits::ActiveModel {
            id: Set(Uuid::new_v4()),
            person_id: Set(person_id),
            album_id: Set(Some(album_id)),
            role: Set("artist".to_string()),
            ..Default::default()
        };
        media_credits::Entity::insert(active).exec(db).await?;
        Ok(())
    }

    /// Upsert a MusicTrack record.
    async fn upsert_track(
        db: &DatabaseConnection,
        album_id: Uuid,
        file: &CollectedAudioFile,
    ) -> Result<Uuid, AppError> {
        let file_name = file
            .file_path
            .rsplit('/')
            .next()
            .unwrap_or(&file.file_path);
        let parent_dir = file.dir_path.rsplit('/').next();

        let (parsed_track_num, parsed_title, _, _) =
            Self::parse_music_filename(file_name, parent_dir);

        let track_title = file
            .tags
            .as_ref()
            .and_then(|t| t.title.clone())
            .or(parsed_title)
            .unwrap_or_else(|| {
                // Fallback: filename without extension
                let dot = file_name.rfind('.');
                if let Some(pos) = dot {
                    file_name[..pos].to_string()
                } else {
                    file_name.to_string()
                }
            });

        let track_number = file
            .tags
            .as_ref()
            .and_then(|t| t.track_number)
            .or(parsed_track_num);
        let disc_number = file.tags.as_ref().and_then(|t| t.disc_number);

        // Try to find existing track
        let mut query = music_tracks::Entity::find()
            .filter(music_tracks::Column::AlbumId.eq(album_id))
            .filter(music_tracks::Column::Title.eq(&track_title));
        if let Some(tn) = track_number {
            query = query.filter(music_tracks::Column::TrackNumber.eq(tn));
        }
        if let Some(dn) = disc_number {
            query = query.filter(music_tracks::Column::DiscNumber.eq(dn));
        }
        let existing = query.one(db).await?;

        if let Some(existing) = existing {
            // Update metadata if available from tags
            if let Some(ref tags) = file.tags {
                let mut active: music_tracks::ActiveModel = existing.clone().into();
                let mut changed = false;
                if tags.disc_number.is_some() && existing.disc_number != tags.disc_number {
                    active.disc_number = Set(tags.disc_number);
                    changed = true;
                }
                if tags.duration.is_some() && existing.duration != tags.duration {
                    active.duration = Set(tags.duration);
                    changed = true;
                }
                if tags.genre.is_some() && existing.genre != tags.genre {
                    active.genre = Set(tags.genre.clone());
                    changed = true;
                }
                if tags.bitrate.is_some() && existing.bitrate != tags.bitrate {
                    active.bitrate = Set(tags.bitrate);
                    changed = true;
                }
                if tags.sample_rate.is_some() && existing.sample_rate != tags.sample_rate {
                    active.sample_rate = Set(tags.sample_rate);
                    changed = true;
                }
                if tags.codec.is_some() && existing.codec != tags.codec {
                    active.codec = Set(tags.codec.clone());
                    changed = true;
                }
                if changed {
                    active.update(db).await?;
                }
            }
            return Ok(existing.id);
        }

        // Check mbTrackId uniqueness before creating
        let safe_mb_track_id = if let Some(ref mb_id) =
            file.tags.as_ref().and_then(|t| t.mb_track_id.clone())
        {
            let conflict = music_tracks::Entity::find()
                .filter(music_tracks::Column::MbTrackId.eq(mb_id.as_str()))
                .one(db)
                .await?;
            if conflict.is_none() {
                Some(mb_id.clone())
            } else {
                None
            }
        } else {
            None
        };

        let id = Uuid::new_v4();
        let active = music_tracks::ActiveModel {
            id: Set(id),
            album_id: Set(album_id),
            title: Set(track_title),
            track_number: Set(track_number),
            disc_number: Set(disc_number),
            duration: Set(file.tags.as_ref().and_then(|t| t.duration)),
            genre: Set(file.tags.as_ref().and_then(|t| t.genre.clone())),
            bitrate: Set(file.tags.as_ref().and_then(|t| t.bitrate)),
            sample_rate: Set(file.tags.as_ref().and_then(|t| t.sample_rate)),
            codec: Set(file.tags.as_ref().and_then(|t| t.codec.clone())),
            mb_track_id: Set(safe_mb_track_id),
            ..Default::default()
        };
        music_tracks::Entity::insert(active).exec(db).await?;
        Ok(id)
    }

    /// Upsert a MediaFile record linked to a music track.
    async fn upsert_music_media_file(
        db: &DatabaseConnection,
        file: &CollectedAudioFile,
        track_id: Uuid,
    ) -> Result<(), AppError> {
        let checksum = format!("{}:{}", file.file_size, file.mtime);
        let file_name = file
            .file_path
            .rsplit('/')
            .next()
            .unwrap_or(&file.file_path);
        let mime_type = Self::audio_mime_type(&file.file_path);
        let now = Utc::now().fixed_offset();

        let existing = media_files::Entity::find()
            .filter(media_files::Column::SourceId.eq(file.source_id))
            .filter(media_files::Column::Path.eq(&file.file_path))
            .one(db)
            .await?;

        if let Some(existing) = existing {
            if existing.checksum.as_deref() == Some(&checksum)
                && existing.track_id == Some(track_id)
            {
                return Ok(());
            }
            let mut active: media_files::ActiveModel = existing.into();
            active.checksum = Set(Some(checksum));
            active.track_id = Set(Some(track_id));
            active.size = Set(Some(file.file_size as i64));
            active.mime_type = Set(Some(mime_type.to_string()));
            active.duration = Set(file.tags.as_ref().and_then(|t| t.duration));
            active.filename = Set(file_name.to_string());
            active.scanned_at = Set(Some(now));
            active.updated_at = Set(Some(now));
            active.update(db).await?;
            return Ok(());
        }

        let active = media_files::ActiveModel {
            id: Set(Uuid::new_v4()),
            source_id: Set(Some(file.source_id)),
            path: Set(file.file_path.clone()),
            filename: Set(file_name.to_string()),
            size: Set(Some(file.file_size as i64)),
            mime_type: Set(Some(mime_type.to_string())),
            duration: Set(file.tags.as_ref().and_then(|t| t.duration)),
            checksum: Set(Some(checksum)),
            track_id: Set(Some(track_id)),
            scanned_at: Set(Some(now)),
            created_at: Set(Some(now)),
            ..Default::default()
        };
        media_files::Entity::insert(active).exec(db).await?;
        Ok(())
    }

    /// Update album metadata after all tracks have been processed.
    async fn update_album_metadata(
        db: &DatabaseConnection,
        album_id: Uuid,
        group: &AlbumGroup,
        is_local: bool,
        vfs: Option<&Arc<Vfs>>,
    ) -> Result<(), AppError> {
        let max_disc = group
            .files
            .iter()
            .filter_map(|f| f.tags.as_ref().and_then(|t| t.disc_number))
            .max()
            .unwrap_or(1);

        let mb_album_id = group
            .files
            .iter()
            .find_map(|f| f.tags.as_ref().and_then(|t| t.mb_album_id.clone()));

        // Check mbAlbumId uniqueness
        let safe_mb_album_id = if let Some(ref mb_id) = mb_album_id {
            let conflict = music_albums::Entity::find()
                .filter(music_albums::Column::MbAlbumId.eq(mb_id.as_str()))
                .one(db)
                .await?;
            if conflict.is_none() || conflict.map(|c| c.id) == Some(album_id) {
                Some(mb_id.clone())
            } else {
                None
            }
        } else {
            None
        };

        let now = Utc::now().fixed_offset();
        let metadata = if !is_local {
            Some(json!({"needsTagRead": true}))
        } else {
            None
        };

        let album = music_albums::Entity::find_by_id(album_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("album {album_id} not found")))?;

        let mut active: music_albums::ActiveModel = album.into();
        active.total_tracks = Set(Some(group.files.len() as i32));
        active.total_discs = Set(Some(max_disc));
        active.updated_at = Set(Some(now));
        if group.year.is_some() {
            active.year = Set(group.year);
        }
        if safe_mb_album_id.is_some() {
            active.mb_album_id = Set(safe_mb_album_id);
        }
        if let Some(meta) = metadata {
            active.metadata = Set(Some(meta));
        }
        active.update(db).await?;

        // Try to find local cover art
        if is_local {
            if let Some(vfs) = vfs {
                for cover_name in Self::COVER_ART_NAMES {
                    let cover_path = format!(
                        "{}/{}",
                        group.dir_path.trim_end_matches('/'),
                        cover_name
                    );
                    match vfs.stat(std::path::Path::new(&cover_path)).await {
                        Ok(_) => {
                            // Store VFS-relative cover path
                            let album = music_albums::Entity::find_by_id(album_id)
                                .one(db)
                                .await?;
                            if let Some(album) = album {
                                let mut active: music_albums::ActiveModel = album.into();
                                active.cover_path = Set(Some(cover_path));
                                active.update(db).await?;
                            }
                            break;
                        }
                        Err(_) => continue,
                    }
                }
            }
        }

        Ok(())
    }

    /// Process one album group: create album, artist, credits, tracks, media files.
    async fn process_album_group(
        db: &DatabaseConnection,
        app_id: Uuid,
        group: &AlbumGroup,
        is_local: bool,
        vfs: Option<&Arc<Vfs>>,
    ) -> Result<(), AppError> {
        let album_id = Self::find_or_create_album(db, app_id, group).await?;
        let person_id = Self::find_or_create_person(db, &group.artist_name).await?;
        Self::ensure_artist_credit(db, album_id, person_id).await?;

        for file in &group.files {
            match Self::upsert_track(db, album_id, file).await {
                Ok(track_id) => {
                    if let Err(e) = Self::upsert_music_media_file(db, file, track_id).await {
                        error!(
                            "Failed to upsert media file \"{}\": {}",
                            file.file_path
                                .rsplit('/')
                                .next()
                                .unwrap_or(&file.file_path),
                            e
                        );
                    }
                }
                Err(e) => {
                    error!(
                        "Track upsert failed \"{}\": {}",
                        file.file_path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&file.file_path),
                        e
                    );
                }
            }
        }

        Self::update_album_metadata(db, album_id, group, is_local, vfs).await?;
        Ok(())
    }

    /// Full music sync for a single file-system source.
    async fn sync_music_source(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        app_id: Uuid,
        source: &file_systems::Model,
        root_path: &str,
    ) -> Result<u64, AppError> {
        let source_type = &source.r#type;
        let is_local = source_type == "local";
        let is_remote = is_remote_fs_type(source_type);

        if !is_local && !is_remote {
            warn!(
                "Unsupported source type \"{}\" for music source \"{}\", skipping",
                source_type, source.name
            );
            return Ok(0);
        }

        let source_id_str = source.id.to_string();
        let vfs = sources.ensure_vfs(&source_id_str).await.map_err(|e| {
            AppError::Internal(format!(
                "Failed to get VFS for source {} ({}): {}",
                source.name, source_id_str, e
            ))
        })?;

        let vfs_root = to_vfs_path(root_path, source);

        // Walk audio files
        let (tx, mut rx) = mpsc::channel::<crate::handlers::media::fs::VideoFileInfo>(256);
        let walk_root = vfs_root.clone();
        let walk_source_id = source_id_str.clone();
        let walk_vfs = vfs.clone();
        let walk_handle = tokio::spawn(async move {
            walk_files_streaming(walk_vfs, &walk_root, &walk_source_id, &AUDIO_EXTENSIONS, tx)
                .await
        });

        // Collect audio files
        let source_id = source.id;
        let mut collected: Vec<CollectedAudioFile> = Vec::new();
        let mut seen_paths: HashSet<String> = HashSet::new();

        while let Some(audio_file) = rx.recv().await {
            seen_paths.insert(audio_file.file_path.clone());
            let checksum = format!("{}:{}", audio_file.file_size, audio_file.mtime);

            // Skip unchanged files
            let existing = media_files::Entity::find()
                .filter(media_files::Column::SourceId.eq(source_id))
                .filter(media_files::Column::Path.eq(&audio_file.file_path))
                .filter(media_files::Column::TrackId.is_not_null())
                .one(db)
                .await?;

            if let Some(ref ex) = existing {
                if ex.checksum.as_deref() == Some(&checksum) {
                    continue;
                }
            }

            // Read tags for local sources using lofty (in blocking task)
            let tags = if is_local {
                // Resolve full filesystem path for local tag reading
                let driver_root = source
                    .config
                    .as_ref()
                    .and_then(|c| {
                        c.get("root")
                            .or_else(|| c.get("root_folder_path"))
                            .or_else(|| c.get("path"))
                    })
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let full_path = if audio_file.file_path.starts_with('/') {
                    format!(
                        "{}{}",
                        driver_root.trim_end_matches('/'),
                        &audio_file.file_path
                    )
                } else {
                    format!(
                        "{}/{}",
                        driver_root.trim_end_matches('/'),
                        &audio_file.file_path
                    )
                };
                let path = std::path::PathBuf::from(&full_path);
                tokio::task::spawn_blocking(move || Self::read_audio_tags(&path))
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            };

            collected.push(CollectedAudioFile {
                file_path: audio_file.file_path,
                dir_path: audio_file.dir_path,
                file_size: audio_file.file_size,
                mtime: audio_file.mtime,
                source_id,
                tags,
            });
        }

        // Wait for walk to complete
        let walk_stats = walk_handle
            .await
            .map_err(|e| {
                AppError::Internal(format!(
                    "Walk task panicked for music source \"{}\": {}",
                    source.name, e
                ))
            })?
            .map_err(|e| {
                AppError::Internal(format!(
                    "Failed to walk music source \"{}\" root={}: {}",
                    source.name, vfs_root, e
                ))
            })?;

        info!(
            "[{}({})] Music walk done: {} dirs, {} audio files found, {} new/changed",
            source.name, source_type, walk_stats.visited_dirs, walk_stats.found_videos,
            collected.len()
        );

        if collected.is_empty() {
            // Still run cleanup even if no new files
            Self::cleanup_missing_music_files(db, app_id, source_id, &vfs_root, &seen_paths)
                .await?;
            return Ok(0);
        }

        // Group into albums
        let album_groups = Self::group_files_into_albums(collected);
        info!(
            "Music sync: {} files grouped into {} albums",
            seen_paths.len(),
            album_groups.len()
        );

        // Process each album group
        let vfs_ref = if is_local { Some(&vfs) } else { None };
        let mut processed_albums = 0u64;
        for (i, group) in album_groups.iter().enumerate() {
            if let Err(e) = Self::process_album_group(db, app_id, group, is_local, vfs_ref).await {
                error!(
                    "Album processing failed \"{}\" by \"{}\": {}",
                    group.album_title, group.artist_name, e
                );
            }
            processed_albums += 1;
            if (i + 1) % 10 == 0 {
                info!(
                    "Music sync progress: {}/{} albums processed",
                    i + 1,
                    album_groups.len()
                );
            }
        }

        // Cleanup missing files
        Self::cleanup_missing_music_files(db, app_id, source_id, &vfs_root, &seen_paths)
            .await?;

        info!(
            "[{}({})] Music sync done: {} albums processed",
            source.name, source_type, processed_albums
        );

        // Return number of new/changed files processed (not job count, since music
        // sync processes inline rather than dispatching jobs)
        Ok(processed_albums)
    }

    /// Remove music-related DB records for files no longer on disk.
    async fn cleanup_missing_music_files(
        db: &DatabaseConnection,
        _app_id: Uuid,
        source_id: Uuid,
        root_path: &str,
        seen_paths: &HashSet<String>,
    ) -> Result<(), AppError> {
        let normalized_root = root_path.trim_end_matches('/');
        let prefix = format!("{}/", normalized_root);

        // Find all music media_files for this source under root_path
        let db_files = media_files::Entity::find()
            .filter(media_files::Column::SourceId.eq(source_id))
            .filter(media_files::Column::TrackId.is_not_null())
            .filter(
                sea_orm::Condition::any()
                    .add(media_files::Column::Path.eq(normalized_root))
                    .add(media_files::Column::Path.starts_with(&prefix)),
            )
            .all(db)
            .await?;

        let stale_files: Vec<&media_files::Model> = db_files
            .iter()
            .filter(|f| !seen_paths.contains(&f.path))
            .collect();

        if stale_files.is_empty() {
            return Ok(());
        }

        info!(
            "Cleaning up {} missing music files (source={}, root={})",
            stale_files.len(),
            source_id,
            root_path
        );

        let stale_file_ids: Vec<Uuid> = stale_files.iter().map(|f| f.id).collect();
        let track_ids: HashSet<Uuid> = stale_files.iter().filter_map(|f| f.track_id).collect();

        // Delete stale media files
        media_files::Entity::delete_many()
            .filter(media_files::Column::Id.is_in(stale_file_ids))
            .exec(db)
            .await?;

        // Cascade: delete orphan tracks (no remaining files)
        let mut album_ids: HashSet<Uuid> = HashSet::new();
        for track_id in &track_ids {
            let remaining = media_files::Entity::find()
                .filter(media_files::Column::TrackId.eq(*track_id))
                .count(db)
                .await?;
            if remaining == 0 {
                if let Some(track) = music_tracks::Entity::find_by_id(*track_id)
                    .one(db)
                    .await?
                {
                    album_ids.insert(track.album_id);
                    music_tracks::Entity::delete_by_id(*track_id)
                        .exec(db)
                        .await?;
                }
            }
        }

        // Cascade: delete orphan albums (no remaining tracks)
        for album_id in &album_ids {
            let remaining = music_tracks::Entity::find()
                .filter(music_tracks::Column::AlbumId.eq(*album_id))
                .count(db)
                .await?;
            if remaining == 0 {
                music_albums::Entity::delete_by_id(*album_id)
                    .exec(db)
                    .await?;
            }
        }

        Ok(())
    }

    // ── find existing media file ────────────────────────────────────────

    /// Look up an existing `media_files` record for the given source + path,
    /// scoped to the library type (movie vs tv).
    async fn find_existing_media_file(
        db: &DatabaseConnection,
        source_id: Uuid,
        file_path: &str,
        is_movie: bool,
        is_tv: bool,
    ) -> Result<Option<media_files::Model>, AppError> {
        let mut query = media_files::Entity::find()
            .filter(media_files::Column::SourceId.eq(source_id))
            .filter(media_files::Column::Path.eq(file_path));

        if is_movie {
            query = query.filter(media_files::Column::MovieId.is_not_null());
        } else if is_tv {
            query = query.filter(media_files::Column::EpisodeId.is_not_null());
        }

        Ok(query.one(db).await?)
    }

    // ── artwork backfill check ──────────────────────────────────────────

    /// Returns `true` if the linked movie/episode is missing poster artwork
    /// and the file should be re-scraped.
    async fn needs_artwork_backfill(
        db: &DatabaseConnection,
        file: &media_files::Model,
        is_movie: bool,
        is_tv: bool,
    ) -> Result<bool, AppError> {
        if is_movie {
            if let Some(movie_id) = file.movie_id {
                let movie = movies::Entity::find_by_id(movie_id).one(db).await?;
                if let Some(movie) = movie {
                    return Ok(movie.poster_path.is_none());
                }
            }
        } else if is_tv {
            if let Some(episode_id) = file.episode_id {
                let episode = episodes::Entity::find_by_id(episode_id).one(db).await?;
                if let Some(episode) = episode {
                    let tv_show = tv_shows::Entity::find_by_id(episode.tv_show_id)
                        .one(db)
                        .await?;
                    if let Some(tv_show) = tv_show {
                        return Ok(tv_show.poster_path.is_none());
                    }
                }
            }
        }
        Ok(false)
    }

    // ── reset media file link ───────────────────────────────────────────

    /// When a file's checksum changed, clear its linked movie/episode so it
    /// gets re-scraped.
    async fn reset_media_file_link(
        db: &DatabaseConnection,
        file_id: Uuid,
        new_checksum: &str,
    ) -> Result<(), AppError> {
        let model = media_files::Entity::find_by_id(file_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("media file {file_id} not found")))?;

        let mut active: media_files::ActiveModel = model.into();
        active.checksum = Set(Some(new_checksum.to_string()));
        active.movie_id = Set(None);
        active.episode_id = Set(None);
        active.scanned_at = Set(None);
        active.updated_at = Set(Some(Utc::now().fixed_offset()));
        active.update(db).await?;

        Ok(())
    }

    // ── missing file cleanup ────────────────────────────────────────────

    /// Delete media_files that are in DB but were NOT found during the walk.
    /// Also cascade-deletes orphan movies/episodes/seasons/tv_shows.
    async fn cleanup_missing_files(
        db: &DatabaseConnection,
        _app_id: Uuid,
        source_id: Uuid,
        _source_type: &str,
        root_path: &str,
        seen_paths: &HashSet<String>,
    ) -> Result<(), AppError> {
        // Find all files in DB for this source under root_path
        let normalized_root = root_path.trim_end_matches('/');
        let prefix = format!("{}/", normalized_root);

        let db_files = media_files::Entity::find()
            .filter(media_files::Column::SourceId.eq(source_id))
            .filter(
                sea_orm::Condition::any()
                    .add(media_files::Column::Path.eq(normalized_root))
                    .add(media_files::Column::Path.starts_with(&prefix)),
            )
            .all(db)
            .await?;

        // Collect IDs of files no longer on disk
        let stale_file_ids: Vec<Uuid> = db_files
            .iter()
            .filter(|f| !seen_paths.contains(&f.path))
            .map(|f| f.id)
            .collect();

        if stale_file_ids.is_empty() {
            return Ok(());
        }

        info!(
            "Cleaning up {} missing files (source={}, root={})",
            stale_file_ids.len(),
            source_id,
            root_path
        );

        // Collect related IDs for cascade cleanup
        let stale_files: Vec<&media_files::Model> = db_files
            .iter()
            .filter(|f| stale_file_ids.contains(&f.id))
            .collect();

        let movie_ids: HashSet<Uuid> = stale_files.iter().filter_map(|f| f.movie_id).collect();
        let episode_ids: HashSet<Uuid> = stale_files.iter().filter_map(|f| f.episode_id).collect();

        // Delete the stale media files
        media_files::Entity::delete_many()
            .filter(media_files::Column::Id.is_in(stale_file_ids.clone()))
            .exec(db)
            .await?;

        // Cascade: delete orphan movies (no remaining files)
        for movie_id in &movie_ids {
            let remaining = media_files::Entity::find()
                .filter(media_files::Column::MovieId.eq(*movie_id))
                .count(db)
                .await?;
            if remaining == 0 {
                movies::Entity::delete_by_id(*movie_id).exec(db).await?;
            }
        }

        // Cascade: delete orphan episodes → seasons → tv shows
        let mut season_ids = HashSet::new();
        let mut tv_show_ids = HashSet::new();

        for episode_id in &episode_ids {
            let remaining = media_files::Entity::find()
                .filter(media_files::Column::EpisodeId.eq(*episode_id))
                .count(db)
                .await?;
            if remaining == 0 {
                if let Some(ep) = episodes::Entity::find_by_id(*episode_id).one(db).await? {
                    season_ids.insert(ep.season_id);
                    tv_show_ids.insert(ep.tv_show_id);
                    episodes::Entity::delete_by_id(*episode_id).exec(db).await?;
                }
            }
        }

        for season_id in &season_ids {
            let remaining = episodes::Entity::find()
                .filter(episodes::Column::SeasonId.eq(*season_id))
                .count(db)
                .await?;
            if remaining == 0 {
                seasons::Entity::delete_by_id(*season_id).exec(db).await?;
            }
        }

        for tv_show_id in &tv_show_ids {
            let remaining = episodes::Entity::find()
                .filter(episodes::Column::TvShowId.eq(*tv_show_id))
                .count(db)
                .await?;
            if remaining == 0 {
                tv_shows::Entity::delete_by_id(*tv_show_id).exec(db).await?;
            }
        }

        Ok(())
    }

    /// Remove photos from the DB that no longer exist on disk.
    async fn cleanup_missing_photos(
        db: &DatabaseConnection,
        app_id: Uuid,
        source_id: Uuid,
        root_path: &str,
        seen_paths: &HashSet<String>,
    ) -> Result<(), AppError> {
        let normalized_root = root_path.trim_end_matches('/');
        let prefix = format!("{}/", normalized_root);

        let db_photos = photos::Entity::find()
            .filter(photos::Column::AppId.eq(app_id))
            .filter(photos::Column::SourceId.eq(source_id))
            .filter(
                sea_orm::Condition::any()
                    .add(photos::Column::Path.eq(normalized_root))
                    .add(photos::Column::Path.starts_with(&prefix)),
            )
            .all(db)
            .await?;

        let stale_ids: Vec<Uuid> = db_photos
            .iter()
            .filter(|p| !seen_paths.contains(&p.path))
            .map(|p| p.id)
            .collect();

        if stale_ids.is_empty() {
            return Ok(());
        }

        info!(
            "Cleaning up {} missing photos (source={}, root={})",
            stale_ids.len(),
            source_id,
            root_path
        );

        photos::Entity::delete_many()
            .filter(photos::Column::Id.is_in(stale_ids))
            .exec(db)
            .await?;

        Ok(())
    }

    // ── media server sync ───────────────────────────────────────────────

    async fn sync_media_server(
        db: &DatabaseConnection,
        http_client: reqwest::Client,
        server: &media_servers::Model,
        server_app_id: &str,
        app_id: Uuid,
        lib_type: &str,
        is_movie: bool,
        is_tv: bool,
        last_sync_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
    ) -> Result<u64, AppError> {
        let server_type = &server.r#type;
        info!(
            "Syncing media server \"{}\" ({}, serverLibraryId={})",
            server.name, server_type, server_app_id
        );

        match server_type.as_str() {
            "plex" => {
                Self::sync_plex_server(
                    db,
                    http_client,
                    server,
                    server_app_id,
                    app_id,
                    lib_type,
                    is_movie,
                    last_sync_at,
                )
                .await
            }
            "emby" => {
                Self::sync_emby_server(
                    db,
                    http_client,
                    server,
                    server_app_id,
                    app_id,
                    lib_type,
                    is_movie,
                    is_tv,
                    last_sync_at,
                )
                .await
            }
            "jellyfin" => {
                Self::sync_jellyfin_server(
                    db,
                    http_client,
                    server,
                    server_app_id,
                    app_id,
                    lib_type,
                    is_movie,
                    is_tv,
                    last_sync_at,
                )
                .await
            }
            other => {
                warn!("Unsupported media server type \"{other}\", skipping");
                Ok(0)
            }
        }
    }

    // ── Plex ────────────────────────────────────────────────────────────

    async fn sync_plex_server(
        db: &DatabaseConnection,
        http_client: reqwest::Client,
        server: &media_servers::Model,
        server_app_id: &str,
        app_id: Uuid,
        lib_type: &str,
        is_movie: bool,
        last_sync_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
    ) -> Result<u64, AppError> {
        let base_url = &server.url;
        let token = server.token.as_deref().unwrap_or("");

        let client = PlexClient::new(http_client, base_url.clone(), token.to_string());
        let mut total_jobs = 0u64;
        let mut start: u32 = 0;

        loop {
            let items = client
                .get_library_items(server_app_id, start, SERVER_SYNC_PAGE_SIZE)
                .await
                .map_err(|e| {
                    AppError::Internal(format!(
                        "Plex get_library_items failed for server \"{}\": {}",
                        server.name, e
                    ))
                })?;

            if items.is_empty() {
                break;
            }

            let count = items.len() as u32;
            info!(
                "  Plex batch: start={start}, fetched={count} items from \"{}\"",
                server.name
            );

            let jobs_batch = Self::build_server_item_jobs(
                &items,
                server,
                app_id,
                lib_type,
                "plex",
                is_movie,
                base_url,
                token,
                "",
                last_sync_at,
            );
            if !jobs_batch.is_empty() {
                total_jobs += JobRepo::create_jobs_batch(db, jobs_batch).await?;
            }

            if count < SERVER_SYNC_PAGE_SIZE {
                break;
            }
            start += count;
        }

        info!(
            "  Plex sync done for \"{}\": {} jobs",
            server.name, total_jobs
        );
        Ok(total_jobs)
    }

    // ── Emby ────────────────────────────────────────────────────────────

    async fn sync_emby_server(
        db: &DatabaseConnection,
        http_client: reqwest::Client,
        server: &media_servers::Model,
        server_app_id: &str,
        app_id: Uuid,
        lib_type: &str,
        is_movie: bool,
        _is_tv: bool,
        last_sync_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
    ) -> Result<u64, AppError> {
        let base_url = &server.url;
        let api_key = server.api_key.as_deref().unwrap_or("");

        let client = EmbyClient::new(http_client, base_url.clone(), api_key.to_string());
        Self::sync_emby_jellyfin_inner(
            db,
            &client,
            server,
            server_app_id,
            app_id,
            lib_type,
            "emby",
            is_movie,
            base_url,
            api_key,
            last_sync_at,
        )
        .await
    }

    // ── Jellyfin ────────────────────────────────────────────────────────

    async fn sync_jellyfin_server(
        db: &DatabaseConnection,
        http_client: reqwest::Client,
        server: &media_servers::Model,
        server_app_id: &str,
        app_id: Uuid,
        lib_type: &str,
        is_movie: bool,
        _is_tv: bool,
        last_sync_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
    ) -> Result<u64, AppError> {
        let base_url = &server.url;
        let api_key = server.api_key.as_deref().unwrap_or("");

        let client = JellyfinClient::new(http_client, base_url.clone(), api_key.to_string());
        Self::sync_emby_jellyfin_inner(
            db,
            &client,
            server,
            server_app_id,
            app_id,
            lib_type,
            "jellyfin",
            is_movie,
            base_url,
            api_key,
            last_sync_at,
        )
        .await
    }

    // ── shared Emby / Jellyfin paginator ────────────────────────────────

    /// Both Emby and Jellyfin wrappers delegate to `get_library_items` with
    /// the same `(library_key, start, size)` signature, so we share the
    /// pagination loop here.
    async fn sync_emby_jellyfin_inner<C: EmbyJellyfinLike>(
        db: &DatabaseConnection,
        client: &C,
        server: &media_servers::Model,
        server_app_id: &str,
        app_id: Uuid,
        lib_type: &str,
        source_type: &str,
        is_movie: bool,
        base_url: &str,
        api_key: &str,
        last_sync_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
    ) -> Result<u64, AppError> {
        let mut total_jobs = 0u64;
        let mut start: u32 = 0;

        loop {
            let items = client
                .get_library_items(server_app_id, start, SERVER_SYNC_PAGE_SIZE)
                .await
                .map_err(|e| {
                    AppError::Internal(format!(
                        "{} get_library_items failed for server \"{}\": {}",
                        source_type, server.name, e
                    ))
                })?;

            if items.is_empty() {
                break;
            }

            let count = items.len() as u32;
            info!(
                "  {source_type} batch: start={start}, fetched={count} items from \"{}\"",
                server.name
            );

            let jobs_batch = Self::build_server_item_jobs(
                &items,
                server,
                app_id,
                lib_type,
                source_type,
                is_movie,
                base_url,
                "",
                api_key,
                last_sync_at,
            );
            if !jobs_batch.is_empty() {
                total_jobs += JobRepo::create_jobs_batch(db, jobs_batch).await?;
            }

            if count < SERVER_SYNC_PAGE_SIZE {
                break;
            }
            start += count;
        }

        info!(
            "  {source_type} sync done for \"{}\": {} jobs",
            server.name, total_jobs
        );
        Ok(total_jobs)
    }

    // ── build job records for server items ───────────────────────────────

    fn build_server_item_jobs<'a>(
        items: &[MediaItem],
        server: &media_servers::Model,
        app_id: Uuid,
        lib_type: &'a str,
        source_type: &'a str,
        is_movie: bool,
        base_url: &str,
        token: &str,
        api_key: &str,
        _last_sync_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
    ) -> Vec<(&'a str, serde_json::Value, Option<serde_json::Value>)> {
        items
            .iter()
            .map(|item| {
                let payload = json!({
                    "item": item,
                    "mediaServerId": server.id.to_string(),
                    "appId": app_id.to_string(),
                    "libType": lib_type,
                    "sourceType": source_type,
                    "baseUrl": base_url,
                    "token": token,
                    "apiKey": api_key,
                    "isMovie": is_movie,
                });
                ("media_server_item_sync", payload, None)
            })
            .collect()
    }
}

// ── helper trait for shared Emby/Jellyfin pagination ────────────────────

/// Minimal trait so `sync_emby_jellyfin_inner` can accept both
/// `EmbyClient` and `JellyfinClient` without boxing.
#[allow(async_fn_in_trait)]
trait EmbyJellyfinLike {
    async fn get_library_items(
        &self,
        library_key: &str,
        start: u32,
        size: u32,
    ) -> Result<Vec<MediaItem>, rust_client_api::ClientError>;
}

impl EmbyJellyfinLike for EmbyClient {
    async fn get_library_items(
        &self,
        library_key: &str,
        start: u32,
        size: u32,
    ) -> Result<Vec<MediaItem>, rust_client_api::ClientError> {
        self.get_library_items(library_key, start, size).await
    }
}

impl EmbyJellyfinLike for JellyfinClient {
    async fn get_library_items(
        &self,
        library_key: &str,
        start: u32,
        size: u32,
    ) -> Result<Vec<MediaItem>, rust_client_api::ClientError> {
        self.get_library_items(library_key, start, size).await
    }
}
