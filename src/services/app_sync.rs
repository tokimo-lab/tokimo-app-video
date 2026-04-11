use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use next_fs::Vfs;
use regex::Regex;
use sea_orm::*;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::db::entities::{
    episodes, vfs, music_album_artists, music_artists, video_files, music_files, book_files, books,
    video_items, music_albums, music_tracks, photo_albums, photo_persons, photos, seasons, tv_shows, videos, musics,
};
use crate::db::repos::job_repo::JobRepo;
use crate::db::repos::media::VideoRepo;
use crate::db::repos::media::MusicRepo;
use crate::db::repos::book_repo::BookRepo;
use crate::error::AppError;
use crate::error::OptionExt;
use crate::handlers::vfs::ops::{walk_video_files_streaming, walk_files_streaming, AUDIO_EXTENSIONS, BOOK_EXTENSIONS, PHOTO_EXTENSIONS};
use crate::services::media::source::SourceRegistry;

/// Types of media libraries (matches TS `AppType`).
fn is_movie_type(lib_type: &str) -> bool {
    matches!(lib_type, "movie" | "adult" | "custom" | "online_video")
}

fn is_tv_type(lib_type: &str) -> bool {
    matches!(lib_type, "tv" | "anime")
}

/// True for movie/adult libraries that use TMDB and benefit from entity-level grouping.
/// `custom` and `online_video` are excluded — they don't call TMDB and have no shared
/// entity to deduplicate against.
fn is_tmdb_movie_type(lib_type: &str) -> bool {
    matches!(lib_type, "movie" | "adult")
}

/// If the last path component looks like a TV season subdirectory, return the parent
/// as the show-level grouping key; otherwise return `dir_path` unchanged.
///
/// Recognised season patterns (case-insensitive):
///   "Season 1", "S01", "Specials", "OVA", "Extra", "第1季", bare "01"–"99"
fn infer_show_dir(dir_path: &str) -> &str {
    use std::sync::OnceLock;
    static SEASON_RE: OnceLock<Regex> = OnceLock::new();
    let re = SEASON_RE.get_or_init(|| {
        Regex::new(r"(?i)^(s\d+|season\s*\d*|specials?|ova|extras?|第\d+季|\d{1,2})$").unwrap()
    });
    if let Some(pos) = dir_path.rfind('/')
        && re.is_match(&dir_path[pos + 1..])
    {
        return &dir_path[..pos];
    }
    dir_path
}

fn is_music_type(lib_type: &str) -> bool {
    lib_type == "music"
}

fn is_book_type(lib_type: &str) -> bool {
    lib_type == "book"
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

/// Convert an absolute `root_path` from `app_vfs` to a VFS-relative path.
///
/// For local sources the DB may store the full filesystem path
/// (e.g. `/home/william/media/movie`) while the local driver's root is already
/// `/home/william/media`. The VFS expects a path relative to the driver root
/// (e.g. `/movie`), so we strip the driver root prefix.
fn to_vfs_path(root_path: &str, source: &vfs::Model) -> String {
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
    /// Execute sync for a video category.
    ///
    /// Reads from `videos` table and parses sources from JSON column.
    pub async fn execute_video_sync(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        storage: &Arc<dyn crate::services::storage::StorageProvider>,
        video_id: Uuid,
        clear_data: bool,
        _http_client: reqwest::Client,
    ) -> Result<SyncResult, AppError> {
        let video = VideoRepo::get_by_id(db, video_id)
            .await?
            .not_found("video not found")?;

        let lib_type = &video.r#type;
        let is_movie = is_movie_type(lib_type);
        let is_tv = is_tv_type(lib_type);

        info!(
            "Starting video sync for \"{}\" (id={}, type={})",
            video.name, video_id, lib_type
        );

        let result = Self::do_video_sync(
            db, sources, storage, &video, lib_type, is_movie, is_tv, clear_data,
        )
        .await;

        match &result {
            Ok(sync_result) => {
                let now = Utc::now().fixed_offset();
                VideoRepo::update_sync_status(db, video_id, "completed", Some(now)).await?;
                info!(
                    "Video sync completed: \"{}\" — {} jobs dispatched",
                    video.name, sync_result.total_jobs
                );
            }
            Err(err) => {
                error!("Video sync failed for \"{}\": {}", video.name, err);
                let _ = VideoRepo::update_sync_status(db, video_id, "failed", None).await;
            }
        }

        result
    }

    /// Sync a music library. Reads sources from JSON column in `musics` table.
    pub async fn execute_music_sync(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        storage: &Arc<dyn crate::services::storage::StorageProvider>,
        music_id: Uuid,
        clear_data: bool,
    ) -> Result<SyncResult, AppError> {
        let music = MusicRepo::get_by_id(db, music_id)
            .await?
            .not_found("music library not found")?;

        info!(
            "Starting music sync for \"{}\" (id={}, type={})",
            music.name, music_id, music.r#type
        );

        let result = Self::do_music_sync(db, sources, storage, &music, clear_data).await;

        match &result {
            Ok(sync_result) => {
                let now = Utc::now().fixed_offset();
                MusicRepo::update_sync_status(db, music_id, "completed", Some(now)).await?;
                info!(
                    "Music sync completed: \"{}\" — {} jobs dispatched",
                    music.name, sync_result.total_jobs
                );
            }
            Err(err) => {
                error!("Music sync failed for \"{}\": {}", music.name, err);
                let _ = MusicRepo::update_sync_status(db, music_id, "failed", None).await;
            }
        }

        result
    }

    /// Execute sync for a book container.
    ///
    /// Similar to `execute_music_sync` but reads from `books` table.
    pub async fn execute_book_sync(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        storage: &Arc<dyn crate::services::storage::StorageProvider>,
        book_id: Uuid,
        clear_data: bool,
    ) -> Result<SyncResult, AppError> {
        let book = BookRepo::get_container_by_id(db, book_id)
            .await?
            .not_found("book library not found")?;

        let lib_type = &book.r#type;

        info!(
            "Starting book sync for \"{}\" (id={}, type={})",
            book.name, book_id, lib_type
        );

        let result = Self::do_book_sync(db, sources, storage, &book, clear_data).await;

        match &result {
            Ok(sync_result) => {
                let now = Utc::now().fixed_offset();
                BookRepo::update_sync_status(db, book_id, "completed", Some(now)).await?;
                info!(
                    "Book sync completed: \"{}\" — {} jobs dispatched",
                    book.name, sync_result.total_jobs
                );
            }
            Err(err) => {
                error!("Book sync failed for \"{}\": {}", book.name, err);
                let _ = BookRepo::update_sync_status(db, book_id, "failed", None).await;
            }
        }

        result
    }

    /// Core book sync logic: parses sources from JSON and walks each.
    async fn do_book_sync(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        storage: &Arc<dyn crate::services::storage::StorageProvider>,
        book: &books::Model,
        clear_data: bool,
    ) -> Result<SyncResult, AppError> {
        let book_id = book.id;
        let lib_type = &book.r#type;

        if clear_data {
            Self::clear_library_data(db, book_id, lib_type).await?;
        }

        let source_tuples = BookRepo::parse_sources(&book.sources);
        if source_tuples.is_empty() {
            info!("  No sources configured for book library, skipping");
            return Ok(SyncResult { total_jobs: 0 });
        }

        let mut total_jobs = 0u64;

        for (source_id, root_path, _is_default) in &source_tuples {
            let source = vfs::Entity::find_by_id(*source_id).one(db).await?;
            let Some(source) = source else {
                warn!("  Source {source_id} not found, skipping");
                continue;
            };

            let jobs = Self::sync_fs_source(
                db, sources, storage, book_id, lib_type,
                false, false, false, // is_movie, is_tv, is_music = false
                &source, root_path,
            ).await?;
            total_jobs += jobs;
        }

        Ok(SyncResult { total_jobs })
    }

    /// Core music sync logic: parses sources from JSON and walks each.
    async fn do_music_sync(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        storage: &Arc<dyn crate::services::storage::StorageProvider>,
        music: &musics::Model,
        clear_data: bool,
    ) -> Result<SyncResult, AppError> {
        let music_id = music.id;

        if clear_data {
            info!(
                "  Clearing existing albums for music library \"{}\"",
                music.name
            );
            let deleted = music_albums::Entity::delete_many()
                .filter(music_albums::Column::MusicId.eq(music_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} music albums");
        }

        let source_tuples = MusicRepo::parse_sources(&music.sources);
        if source_tuples.is_empty() {
            info!("  No sources configured for music library, skipping");
            return Ok(SyncResult { total_jobs: 0 });
        }

        let mut total_jobs = 0u64;

        for (source_id, root_path, _is_default) in &source_tuples {
            let source = vfs::Entity::find_by_id(*source_id).one(db).await?;
            let Some(source) = source else {
                warn!("  Source {source_id} not found, skipping");
                continue;
            };

            let jobs = Self::sync_music_source(db, sources, storage, music_id, &source, root_path).await?;
            total_jobs += jobs;
        }

        Ok(SyncResult { total_jobs })
    }

    /// Core video sync logic: parses sources from JSON and walks each.
    #[allow(clippy::too_many_arguments)]
    async fn do_video_sync(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        storage: &Arc<dyn crate::services::storage::StorageProvider>,
        video: &videos::Model,
        lib_type: &str,
        is_movie: bool,
        is_tv: bool,
        clear_data: bool,
    ) -> Result<SyncResult, AppError> {
        let video_id = video.id;

        if clear_data {
            Self::clear_library_data(db, video_id, lib_type).await?;
        }

        let mut total_jobs = 0u64;

        // Parse sources from JSON column
        let source_tuples = VideoRepo::parse_sources(&video.sources);
        for (source_id, root_path, _is_default) in &source_tuples {
            let source = vfs::Entity::find_by_id(*source_id)
                .one(db)
                .await?
                .ok_or_else(|| {
                    AppError::NotFound(format!("source {source_id} not found"))
                })?;

            let jobs = Self::sync_fs_source(
                db,
                sources,
                storage,
                video_id,
                lib_type,
                is_movie,
                is_tv,
                false, // is_music = false for video
                &source,
                root_path,
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
        let mut source_ids: Vec<Uuid> = Vec::new();

        if source_ids.is_empty()
            && let Ok(Some(video)) = videos::Entity::find_by_id(app_id).one(db).await
        {
            source_ids = VideoRepo::parse_sources(&video.sources)
                .into_iter()
                .map(|(sid, _, _)| sid)
                .collect();
        }

        if is_movie_type(lib_type) {
            // Delete media_files linked to movies in this library BEFORE deleting
            // movies (otherwise FK SET NULL leaves orphan rows that block re-scrape).
            let movie_ids: Vec<Uuid> = video_items::Entity::find()
                .filter(video_items::Column::VideoId.eq(app_id))
                .all(db)
                .await?
                .into_iter()
                .map(|m| m.id)
                .collect();
            if !movie_ids.is_empty() {
                let mf_deleted = video_files::Entity::delete_many()
                    .filter(video_files::Column::VideoItemId.is_in(movie_ids.clone()))
                    .exec(db)
                    .await?
                    .rows_affected;
                info!("  Deleted {mf_deleted} video files (linked to movies)");
            }

            let deleted = video_items::Entity::delete_many()
                .filter(video_items::Column::VideoId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} movies");
        } else if is_tv_type(lib_type) {
            // Delete media_files linked to episodes of tv shows in this library.
            let show_ids: Vec<Uuid> = tv_shows::Entity::find()
                .filter(tv_shows::Column::VideoId.eq(app_id))
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
                    let mf_deleted = video_files::Entity::delete_many()
                        .filter(video_files::Column::EpisodeId.is_in(episode_ids))
                        .exec(db)
                        .await?
                        .rows_affected;
                    info!("  Deleted {mf_deleted} video files (linked to episodes)");
                }
            }

            let deleted = tv_shows::Entity::delete_many()
                .filter(tv_shows::Column::VideoId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} tv shows (cascade: seasons + episodes)");
        } else if is_music_type(lib_type) {
            let deleted = music_albums::Entity::delete_many()
                .filter(music_albums::Column::MusicId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} music albums");
        } else if is_book_type(lib_type) {
            use crate::db::entities::{book_chapters, book_items, book_volumes};

            let book_ids: Vec<Uuid> = book_items::Entity::find()
                .filter(book_items::Column::BookId.eq(app_id))
                .all(db)
                .await?
                .into_iter()
                .map(|n| n.id)
                .collect();
            if !book_ids.is_empty() {
                let ch_deleted = book_chapters::Entity::delete_many()
                    .filter(book_chapters::Column::BookId.is_in(book_ids.clone()))
                    .exec(db)
                    .await?
                    .rows_affected;
                info!("  Deleted {ch_deleted} book chapters");

                let vol_deleted = book_volumes::Entity::delete_many()
                    .filter(book_volumes::Column::BookId.is_in(book_ids.clone()))
                    .exec(db)
                    .await?
                    .rows_affected;
                info!("  Deleted {vol_deleted} book volumes");

                let mf_deleted = book_files::Entity::delete_many()
                    .filter(book_files::Column::BookId.is_in(book_ids.clone()))
                    .exec(db)
                    .await?
                    .rows_affected;
                info!("  Deleted {mf_deleted} book files (linked to book items)");
            }

            let deleted = book_items::Entity::delete_many()
                .filter(book_items::Column::BookId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} book items");
        } else if is_photo_type(lib_type) {
            // Delete photo_persons first (faces will cascade from photos, but persons
            // are only linked to appId and won't be cleaned up by photo deletion).
            let persons_deleted = photo_persons::Entity::delete_many()
                .filter(photo_persons::Column::AppId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            if persons_deleted > 0 {
                info!("  Deleted {persons_deleted} photo persons");
            }

            // Delete photo_albums (they become empty after photos are cleared and
            // won't be rebuilt by re-sync).
            let albums_deleted = photo_albums::Entity::delete_many()
                .filter(photo_albums::Column::AppId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            if albums_deleted > 0 {
                info!("  Deleted {albums_deleted} photo albums");
            }

            let deleted = photos::Entity::delete_many()
                .filter(photos::Column::AppId.eq(app_id))
                .exec(db)
                .await?
                .rows_affected;
            info!("  Deleted {deleted} photos");
        }

        // Delete orphaned video_files (unlinked, with movie_id AND episode_id both NULL)
        // that belong to this library's sources.
        if !source_ids.is_empty() && (is_movie_type(lib_type) || is_tv_type(lib_type)) {
            let orphan_deleted = video_files::Entity::delete_many()
                .filter(video_files::Column::SourceId.is_in(source_ids.clone()))
                .filter(video_files::Column::VideoItemId.is_null())
                .filter(video_files::Column::EpisodeId.is_null())
                .exec(db)
                .await?
                .rows_affected;
            if orphan_deleted > 0 {
                info!("  Deleted {orphan_deleted} orphaned video files");
            }
        }
        if !source_ids.is_empty() && is_music_type(lib_type) {
            let orphan_deleted = music_files::Entity::delete_many()
                .filter(music_files::Column::SourceId.is_in(source_ids.clone()))
                .filter(music_files::Column::TrackId.is_null())
                .exec(db)
                .await?
                .rows_affected;
            if orphan_deleted > 0 {
                info!("  Deleted {orphan_deleted} orphaned music files");
            }
        }
        if !source_ids.is_empty() && is_book_type(lib_type) {
            let orphan_deleted = book_files::Entity::delete_many()
                .filter(book_files::Column::SourceId.is_in(source_ids.clone()))
                .filter(book_files::Column::BookId.is_null())
                .exec(db)
                .await?
                .rows_affected;
            if orphan_deleted > 0 {
                info!("  Deleted {orphan_deleted} orphaned book files");
            }
        }

        Ok(())
    }

    // ── file system source sync ─────────────────────────────────────────

    /// Batch size for flushing accumulated jobs to DB.
    const JOB_BATCH_FLUSH_SIZE: usize = 50;

    /// Emit grouped jobs (book dirs, TV shows, movie dirs) accumulated during the walk.
    /// Returns the total number of jobs created.
    #[allow(clippy::too_many_arguments)]
    async fn flush_grouped_jobs<'a>(
        db: &DatabaseConnection,
        jobs_batch: &mut Vec<(&'a str, serde_json::Value, Option<serde_json::Value>)>,
        tv_groups: HashMap<String, Vec<serde_json::Value>>,
        movie_groups: HashMap<String, Vec<serde_json::Value>>,
        book_dir_files: HashMap<String, Vec<crate::handlers::vfs::ops::VideoFileInfo>>,
        app_id: Uuid,
        source_id: Uuid,
        lib_type: &'a str,
    ) -> Result<u64, AppError> {
        let mut total = 0u64;
        let flush_size = Self::JOB_BATCH_FLUSH_SIZE;

        for (dir_path, files) in &book_dir_files {
            let chapter_files: Vec<serde_json::Value> = files
                .iter()
                .map(|f| json!({ "filePath": f.file_path, "fileSize": f.file_size, "checksum": format!("{}:{}", f.file_size, f.mtime) }))
                .collect();
            let total_size: u64 = files.iter().map(|f| f.file_size).sum();
            jobs_batch.push((
                "book_scrape",
                json!({ "dirPath": dir_path, "chapterFiles": chapter_files, "totalSize": total_size, "appId": app_id.to_string(), "sourceId": source_id.to_string(), "libType": lib_type }),
                None,
            ));
            if jobs_batch.len() >= flush_size {
                total += JobRepo::create_jobs_batch(db, std::mem::take(jobs_batch)).await?;
            }
        }

        for (show_dir, files) in tv_groups {
            jobs_batch.push((
                "tv_scrape",
                json!({ "showDir": show_dir, "appId": app_id.to_string(), "sourceId": source_id.to_string(), "libType": lib_type, "files": files }),
                None,
            ));
            if jobs_batch.len() >= flush_size {
                total += JobRepo::create_jobs_batch(db, std::mem::take(jobs_batch)).await?;
            }
        }

        for (movie_dir, files) in movie_groups {
            jobs_batch.push((
                "movie_scrape",
                json!({ "movieDir": movie_dir, "appId": app_id.to_string(), "sourceId": source_id.to_string(), "libType": lib_type, "files": files }),
                None,
            ));
            if jobs_batch.len() >= flush_size {
                total += JobRepo::create_jobs_batch(db, std::mem::take(jobs_batch)).await?;
            }
        }

        Ok(total)
    }

    #[allow(clippy::too_many_arguments)]
    async fn sync_fs_source(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        storage: &Arc<dyn crate::services::storage::StorageProvider>,
        app_id: Uuid,
        lib_type: &str,
        is_movie: bool,
        is_tv: bool,
        is_music: bool,
        source: &vfs::Model,
        root_path: &str,
    ) -> Result<u64, AppError> {
        let source_type = &source.r#type;

        if is_book_type(lib_type) {
            info!(
                "Book app sync: walking file system source \"{}\" for book files",
                source.name
            );
        }

        if is_music {
            return Self::sync_music_source(db, sources, storage, app_id, source, root_path).await;
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
        let (tx, mut rx) = mpsc::channel::<crate::handlers::vfs::ops::VideoFileInfo>(256);
        let walk_root = vfs_root.clone();
        let walk_source_id = source_id_str.clone();
        let is_photo = is_photo_type(lib_type);
        let is_book = is_book_type(lib_type);
        let walk_handle = tokio::spawn(async move {
            if is_photo {
                walk_files_streaming(vfs, &walk_root, &walk_source_id, &PHOTO_EXTENSIONS, tx).await
            } else if is_book {
                walk_files_streaming(vfs, &walk_root, &walk_source_id, &BOOK_EXTENSIONS, tx).await
            } else {
                walk_video_files_streaming(vfs, &walk_root, &walk_source_id, tx).await
            }
        });

        // Consume files as they arrive — check DB + accumulate jobs incrementally
        let source_id = source.id;
        let is_tmdb_movie = is_tmdb_movie_type(lib_type);
        let mut seen_paths = HashSet::new();
        let mut jobs_batch: Vec<(&str, serde_json::Value, Option<serde_json::Value>)> = Vec::new();
        let mut total_jobs = 0u64;
        let mut skipped = 0u64;

        // Entity-level grouping buffers: one job per TV show, one job per movie dir.
        // Files that still need processing (new or changed) are buffered here and
        // emitted as a single batch job after the walk completes.
        let mut tv_groups: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        let mut movie_groups: HashMap<String, Vec<serde_json::Value>> = HashMap::new();

        // For books: buffer .txt files grouped by directory, emit one job per directory.
        // Non-txt book files (epub/mobi/etc.) get individual jobs like before.
        let mut book_dir_files: HashMap<String, Vec<crate::handlers::vfs::ops::VideoFileInfo>> =
            HashMap::new();

        // Pre-load existing photo paths for this source to skip already-indexed photos
        // without creating 170K+ redundant file_scrape jobs every sync.
        let existing_photo_paths: HashSet<String> = if is_photo {
            photos::Entity::find()
                .filter(photos::Column::AppId.eq(app_id))
                .filter(photos::Column::SourceId.eq(source_id))
                .select_only()
                .column(photos::Column::Path)
                .into_tuple::<String>()
                .all(db)
                .await?
                .into_iter()
                .collect()
        } else {
            HashSet::new()
        };

        while let Some(video) = rx.recv().await {
            seen_paths.insert(video.file_path.clone());
            let checksum = format!("{}:{}", video.file_size, video.mtime);

            // Photo libraries: skip already-indexed photos (dedup by path).
            if is_photo && existing_photo_paths.contains(&video.file_path) {
                skipped += 1;
                continue;
            }

            // Video/TV/Movie libraries: check video_files table with checksum.
            if !is_photo && !is_book {
                let existing =
                    Self::find_existing_video_file(db, source_id, &video.file_path, is_movie, is_tv)
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
                        Self::reset_video_file_link(db, existing.id, &checksum).await?;
                    }
                }
            }

            // Book .txt files: group by parent directory for chapter-based books
            if is_book && video.file_path.to_lowercase().ends_with(".txt") {
                book_dir_files
                    .entry(video.dir_path.clone())
                    .or_default()
                    .push(video);
                continue;
            }

            // TV/Anime: buffer by show-level dir (collapse season subdirs to show dir).
            if is_tv {
                let show_dir = infer_show_dir(&video.dir_path).to_string();
                tv_groups.entry(show_dir).or_default().push(json!({
                    "filePath": video.file_path,
                    "dirPath": video.dir_path,
                    "fileSize": video.file_size,
                    "checksum": checksum,
                }));
                continue;
            }

            // Movie/Adult: buffer by directory — one job per movie dir.
            if is_tmdb_movie {
                movie_groups.entry(video.dir_path.clone()).or_default().push(json!({
                    "filePath": video.file_path,
                    "dirPath": video.dir_path,
                    "fileSize": video.file_size,
                    "checksum": checksum,
                }));
                continue;
            }

            // All other types (custom, online_video, photo, book non-txt): per-file job.
            let job_type = if is_book { "book_scrape" } else { "file_scrape" };
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

        // Emit entity-level TV/Movie/Book jobs + flush remaining per-file jobs.
        total_jobs += Self::flush_grouped_jobs(
            db,
            &mut jobs_batch,
            tv_groups,
            movie_groups,
            book_dir_files,
            app_id,
            source_id,
            lib_type,
        )
        .await?;

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
                    tag.get_string(&lofty::tag::ItemKey::AlbumArtist).map(std::string::ToString::to_string),
                    tag.album().map(|s| s.to_string()),
                    tag.track().map(|n| n as i32),
                    tag.disk().map(|n| n as i32),
                    tag.year().map(|n| n as i32),
                    tag.genre().map(|s| s.to_string()),
                    tag.get_string(&lofty::tag::ItemKey::MusicBrainzRecordingId).map(std::string::ToString::to_string),
                    tag.get_string(&lofty::tag::ItemKey::MusicBrainzReleaseId).map(std::string::ToString::to_string),
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
            Some(format!("{file_type:?}"))
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
    /// Strip date-prefix noise from album/folder titles.
    ///
    /// Handles two patterns:
    /// - `"2002年07月18日 - 八度空间"` → `"八度空间"`
    /// - `"2003年11月11日《寻找周杰伦EP》"` → `"寻找周杰伦EP"`
    fn extract_clean_title(title: &str) -> String {
        let t = title.trim();

        // Pattern 1: "YYYY年MM月DD日[ ]- title" or "YYYY年MM月DD日- title"
        // Match everything before " - " or "- " if the prefix is only date characters
        if let Some(pos) = t.find(" - ").or_else(|| {
            // also handle "日-" (no space before dash)
            t.find("日-").map(|p| p + '日'.len_utf8())
        }) {
            let before = &t[..pos];
            let is_date_prefix = !before.is_empty()
                && before.chars().all(|c| {
                    c.is_ascii_digit()
                        || c == '年'
                        || c == '月'
                        || c == '日'
                        || c == '-'
                        || c == ' '
                });
            if is_date_prefix {
                // Skip the separator (either " - " or "- ")
                let rest = if t[pos..].starts_with(" - ") {
                    &t[pos + 3..]
                } else {
                    &t[pos + 1..]
                };
                if !rest.trim().is_empty() {
                    return rest.trim().to_string();
                }
            }
        }

        // Pattern 2: "YYYY年...《title》[optional suffix]"
        if let (Some(start), Some(end)) = (t.find('《'), t.rfind('》')) {
            let pre = &t[..start];
            let is_date_prefix = pre.trim().is_empty()
                || pre.trim().chars().all(|c| {
                    c.is_ascii_digit()
                        || c == '年'
                        || c == '月'
                        || c == '日'
                        || c == '-'
                        || c == ' '
                });
            if is_date_prefix && end > start {
                let inside = &t[start + '《'.len_utf8()..end];
                let suffix = t[end + '》'.len_utf8()..].trim();
                if suffix.is_empty() {
                    return inside.trim().to_string();
                }
                return format!("{} {}", inside.trim(), suffix);
            }
        }

        t.to_string()
    }

    fn get_album_info(file: &CollectedAudioFile) -> (String, String, Option<i32>) {
        if let Some(ref tags) = file.tags
            && let Some(ref album) = tags.album {
                let artist_name = tags
                    .album_artist
                    .clone()
                    .or_else(|| tags.artist.clone())
                    .unwrap_or_else(|| "Unknown Artist".to_string());
                let clean_album = Self::extract_clean_title(album);
                return (artist_name, clean_album, tags.year);
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
        let album_title = Self::extract_clean_title(dir_name);

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

    /// Find or create a `MusicArtist` record by name.
    #[allow(dead_code)]
    async fn find_or_create_music_artist(
        db: &DatabaseConnection,
        name: &str,
    ) -> Result<Uuid, AppError> {
        if let Some(a) = music_artists::Entity::find()
            .filter(music_artists::Column::Name.eq(name))
            .one(db)
            .await?
        {
            return Ok(a.id);
        }

        // Serialise creation: `music_artists.name` has no unique constraint.
        let lock_key = name.to_lowercase();
        let per_lock = {
            let mut locks = crate::MUSIC_ARTIST_CREATION_LOCKS.lock().await;
            locks.entry(lock_key).or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))).clone()
        };
        let _guard = per_lock.lock().await;

        // Re-check under lock.
        if let Some(a) = music_artists::Entity::find()
            .filter(music_artists::Column::Name.eq(name))
            .one(db)
            .await?
        {
            return Ok(a.id);
        }

        let id = Uuid::new_v4();
        let now = Utc::now().fixed_offset();
        let active = music_artists::ActiveModel {
            id: Set(id),
            name: Set(name.to_string()),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        music_artists::Entity::insert(active).exec(db).await?;
        Ok(id)
    }

    /// Find or create a `MusicAlbum` for the given group.
    async fn find_or_create_album(
        db: &DatabaseConnection,
        app_id: Uuid,
        group: &AlbumGroup,
    ) -> Result<Uuid, AppError> {
        // Find existing albums with matching title in this library
        let candidates = music_albums::Entity::find()
            .filter(music_albums::Column::MusicId.eq(app_id))
            .filter(music_albums::Column::Title.eq(&group.album_title))
            .find_with_related(music_album_artists::Entity)
            .all(db)
            .await?;

        // Match by artist name via album_artists → music_artist
        let mut unscraped_stub: Option<Uuid> = None;
        for (album, artists) in &candidates {
            if artists.is_empty() {
                // Stub from a previous sync where scraping failed — reuse it
                unscraped_stub = Some(album.id);
                continue;
            }
            for artist_link in artists {
                if let Some(artist) = music_artists::Entity::find_by_id(artist_link.artist_id)
                    .one(db)
                    .await?
                    && artist.name.to_lowercase() == group.artist_name.to_lowercase() {
                        return Ok(album.id);
                    }
            }
        }
        if let Some(id) = unscraped_stub {
            return Ok(id);
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
            music_id: Set(app_id),
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

    /// Ensure an "artist" link exists between a music artist and an album.
    #[allow(dead_code)]
    async fn ensure_artist_credit(
        db: &DatabaseConnection,
        album_id: Uuid,
        artist_id: Uuid,
    ) -> Result<(), AppError> {
        let existing = music_album_artists::Entity::find()
            .filter(music_album_artists::Column::ArtistId.eq(artist_id))
            .filter(music_album_artists::Column::AlbumId.eq(album_id))
            .filter(music_album_artists::Column::Role.eq("artist"))
            .one(db)
            .await?;
        if existing.is_some() {
            return Ok(());
        }

        let active = music_album_artists::ActiveModel {
            id: Set(Uuid::new_v4()),
            artist_id: Set(artist_id),
            album_id: Set(album_id),
            role: Set("artist".to_string()),
            sort_order: Set(0),
        };
        match music_album_artists::Entity::insert(active).exec(db).await {
            Ok(_) | Err(_) => {} // ignore unique violations
        }
        Ok(())
    }

    /// Upsert a `MusicTrack` record.
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

    /// Upsert a `MediaFile` record linked to a music track.
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

        let existing = music_files::Entity::find()
            .filter(music_files::Column::SourceId.eq(file.source_id))
            .filter(music_files::Column::Path.eq(&file.file_path))
            .one(db)
            .await?;

        if let Some(existing) = existing {
            if existing.checksum.as_deref() == Some(&checksum)
                && existing.track_id == Some(track_id)
            {
                return Ok(());
            }
            let mut active: music_files::ActiveModel = existing.into();
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

        let active = music_files::ActiveModel {
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
        music_files::Entity::insert(active).exec(db).await?;
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
        let metadata = if is_local {
            None
        } else {
            Some(json!({"needsTagRead": true}))
        };

        let album = music_albums::Entity::find_by_id(album_id)
            .one(db)
            .await?
            .not_found(format!("album {album_id} not found"))?;

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
        if is_local
            && let Some(vfs) = vfs {
                for cover_name in Self::COVER_ART_NAMES {
                    let cover_path = format!(
                        "{}/{}",
                        group.dir_path.trim_end_matches('/'),
                        cover_name
                    );
                    if vfs.stat(std::path::Path::new(&cover_path)).await.is_ok() {
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
                }
            }

        Ok(())
    }

    /// Process one album group: create album, artist, credits, tracks, media files.
    async fn process_album_group(
        db: &DatabaseConnection,
        app_id: Uuid,
        _storage: &Arc<dyn crate::services::storage::StorageProvider>,
        group: &AlbumGroup,
        is_local: bool,
        vfs: Option<&Arc<Vfs>>,
    ) -> Result<Uuid, AppError> {
        let album_id = Self::find_or_create_album(db, app_id, group).await?;

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

        Ok(album_id)
    }

    /// Full music sync for a single file-system source.
    async fn sync_music_source(
        db: &DatabaseConnection,
        sources: &SourceRegistry,
        storage: &Arc<dyn crate::services::storage::StorageProvider>,
        app_id: Uuid,
        source: &vfs::Model,
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
        let (tx, mut rx) = mpsc::channel::<crate::handlers::vfs::ops::VideoFileInfo>(256);
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
            let existing = music_files::Entity::find()
                .filter(music_files::Column::SourceId.eq(source_id))
                .filter(music_files::Column::Path.eq(&audio_file.file_path))
                .filter(music_files::Column::TrackId.is_not_null())
                .one(db)
                .await?;

            if let Some(ref ex) = existing
                && ex.checksum.as_deref() == Some(&checksum) {
                    continue;
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

        // Process each album group — enqueue async scrape jobs
        let vfs_ref = if is_local { Some(&vfs) } else { None };
        let mut total_jobs = 0u64;
        let mut scrape_jobs: Vec<(&str, serde_json::Value, Option<serde_json::Value>)> = Vec::new();

        for (i, group) in album_groups.iter().enumerate() {
            match Self::process_album_group(db, app_id, storage, group, is_local, vfs_ref).await {
                Ok(album_id) => {
                    // Check if album needs scraping
                    let already_scraped = music_albums::Entity::find_by_id(album_id)
                        .one(db)
                        .await?
                        .and_then(|a| a.scraped_at)
                        .is_some();
                    if !already_scraped {
                        scrape_jobs.push((
                            "music_scrape",
                            json!({
                                "albumId": album_id.to_string(),
                                "appId": app_id.to_string(),
                            }),
                            None,
                        ));
                        if scrape_jobs.len() >= Self::JOB_BATCH_FLUSH_SIZE {
                            total_jobs += JobRepo::create_jobs_batch(db, std::mem::take(&mut scrape_jobs)).await?;
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Album processing failed \"{}\" by \"{}\": {}",
                        group.album_title, group.artist_name, e
                    );
                }
            }
            if (i + 1) % 10 == 0 {
                info!(
                    "Music sync progress: {}/{} albums processed",
                    i + 1,
                    album_groups.len()
                );
            }
        }

        if !scrape_jobs.is_empty() {
            total_jobs += JobRepo::create_jobs_batch(db, scrape_jobs).await?;
        }

        // Cleanup missing files
        Self::cleanup_missing_music_files(db, app_id, source_id, &vfs_root, &seen_paths)
            .await?;

        info!(
            "[{}({})] Music sync done: {} albums processed, {} scrape jobs enqueued",
            source.name, source_type, album_groups.len(), total_jobs
        );

        // Return number of scrape jobs enqueued
        Ok(total_jobs)
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
        let prefix = format!("{normalized_root}/");

        // Find all music_files for this source under root_path
        let db_files = music_files::Entity::find()
            .filter(music_files::Column::SourceId.eq(source_id))
            .filter(music_files::Column::TrackId.is_not_null())
            .filter(
                sea_orm::Condition::any()
                    .add(music_files::Column::Path.eq(normalized_root))
                    .add(music_files::Column::Path.starts_with(&prefix)),
            )
            .all(db)
            .await?;

        let stale_files: Vec<&music_files::Model> = db_files
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

        // Delete stale music files
        music_files::Entity::delete_many()
            .filter(music_files::Column::Id.is_in(stale_file_ids))
            .exec(db)
            .await?;

        // Cascade: delete orphan tracks (no remaining files)
        let mut album_ids: HashSet<Uuid> = HashSet::new();
        for track_id in &track_ids {
            let remaining = music_files::Entity::find()
                .filter(music_files::Column::TrackId.eq(*track_id))
                .count(db)
                .await?;
            if remaining == 0
                && let Some(track) = music_tracks::Entity::find_by_id(*track_id)
                    .one(db)
                    .await?
                {
                    album_ids.insert(track.album_id);
                    music_tracks::Entity::delete_by_id(*track_id)
                        .exec(db)
                        .await?;
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

    // ── find existing video file ────────────────────────────────────────

    /// Look up an existing `video_files` record for the given source + path,
    /// scoped to the library type (movie vs tv).
    async fn find_existing_video_file(
        db: &DatabaseConnection,
        source_id: Uuid,
        file_path: &str,
        is_movie: bool,
        is_tv: bool,
    ) -> Result<Option<video_files::Model>, AppError> {
        let mut query = video_files::Entity::find()
            .filter(video_files::Column::SourceId.eq(source_id))
            .filter(video_files::Column::Path.eq(file_path));

        if is_movie {
            query = query.filter(video_files::Column::VideoItemId.is_not_null());
        } else if is_tv {
            query = query.filter(video_files::Column::EpisodeId.is_not_null());
        }

        Ok(query.one(db).await?)
    }

    // ── artwork backfill check ──────────────────────────────────────────

    /// Returns `true` if the linked movie/episode is missing poster artwork
    /// and the file should be re-scraped.
    async fn needs_artwork_backfill(
        db: &DatabaseConnection,
        file: &video_files::Model,
        is_movie: bool,
        is_tv: bool,
    ) -> Result<bool, AppError> {
        if is_movie {
            if let Some(movie_id) = file.video_item_id {
                let movie = video_items::Entity::find_by_id(movie_id).one(db).await?;
                if let Some(movie) = movie {
                    return Ok(movie.poster_path.is_none());
                }
            }
        } else if is_tv
            && let Some(episode_id) = file.episode_id {
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
        Ok(false)
    }

    // ── reset video file link ───────────────────────────────────────────

    /// When a file's checksum changed, clear its linked movie/episode so it
    /// gets re-scraped.
    async fn reset_video_file_link(
        db: &DatabaseConnection,
        file_id: Uuid,
        new_checksum: &str,
    ) -> Result<(), AppError> {
        let model = video_files::Entity::find_by_id(file_id)
            .one(db)
            .await?
            .not_found(format!("video file {file_id} not found"))?;

        let mut active: video_files::ActiveModel = model.into();
        active.checksum = Set(Some(new_checksum.to_string()));
        active.video_item_id = Set(None);
        active.episode_id = Set(None);
        active.scanned_at = Set(None);
        active.updated_at = Set(Some(Utc::now().fixed_offset()));
        active.update(db).await?;

        Ok(())
    }

    // ── missing file cleanup ────────────────────────────────────────────

    /// Delete `video_files` that are in DB but were NOT found during the walk.
    /// Also cascade-deletes orphan `movies/episodes/seasons/tv_shows`.
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
        let prefix = format!("{normalized_root}/");

        let db_files = video_files::Entity::find()
            .filter(video_files::Column::SourceId.eq(source_id))
            .filter(
                sea_orm::Condition::any()
                    .add(video_files::Column::Path.eq(normalized_root))
                    .add(video_files::Column::Path.starts_with(&prefix)),
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
        let stale_files: Vec<&video_files::Model> = db_files
            .iter()
            .filter(|f| stale_file_ids.contains(&f.id))
            .collect();

        let movie_ids: HashSet<Uuid> = stale_files.iter().filter_map(|f| f.video_item_id).collect();
        let episode_ids: HashSet<Uuid> = stale_files.iter().filter_map(|f| f.episode_id).collect();

        // Delete the stale video files
        video_files::Entity::delete_many()
            .filter(video_files::Column::Id.is_in(stale_file_ids.clone()))
            .exec(db)
            .await?;

        // Cascade: delete orphan movies (no remaining files)
        for movie_id in &movie_ids {
            let remaining = video_files::Entity::find()
                .filter(video_files::Column::VideoItemId.eq(*movie_id))
                .count(db)
                .await?;
            if remaining == 0 {
                video_items::Entity::delete_by_id(*movie_id).exec(db).await?;
            }
        }

        // Cascade: delete orphan episodes → seasons → tv shows
        let mut season_ids = HashSet::new();
        let mut tv_show_ids = HashSet::new();

        for episode_id in &episode_ids {
            let remaining = video_files::Entity::find()
                .filter(video_files::Column::EpisodeId.eq(*episode_id))
                .count(db)
                .await?;
            if remaining == 0
                && let Some(ep) = episodes::Entity::find_by_id(*episode_id).one(db).await? {
                    season_ids.insert(ep.season_id);
                    tv_show_ids.insert(ep.tv_show_id);
                    episodes::Entity::delete_by_id(*episode_id).exec(db).await?;
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
        let prefix = format!("{normalized_root}/");

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

}

