use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use std::{
    collections::VecDeque,
    path::{Path as StdPath, PathBuf},
    sync::Arc,
    time::Instant,
};
use tokimo_vfs::Vfs;
use tokio::sync::mpsc;
use tracing::debug;

use crate::AppState;
use crate::handlers::{ApiResponse, err400, err404, err500, ok};
use crate::services::media::source::normalize_source_path;

use super::types::{VIDEO_EXTENSIONS, VideoFileInfo, WalkProgress, WalkStats, WalkVideoFilesRequest};

/// Max concurrent `vfs.list()` calls during walk.
const WALK_CONCURRENCY: usize = 8;

// ── HTTP handler (unchanged API) ────────────────────────────────────────

pub async fn walk_vfs_video_files(
    State(state): State<Arc<AppState>>,
    Path(source_id): Path<String>,
    Json(body): Json<WalkVideoFilesRequest>,
) -> Result<Json<ApiResponse<Vec<VideoFileInfo>>>, (StatusCode, Json<ApiResponse<Vec<VideoFileInfo>>>)> {
    let root_path = normalize_source_path(&body.root_path).map_err(err400)?;
    let vfs = state.sources.ensure_vfs(&source_id).await.map_err(err404)?;
    debug!("walk video files source={} root={}", source_id, root_path);
    let videos = walk_video_files(vfs, &root_path, &source_id)
        .await
        .map_err(|err| err500(err.clone()))?;
    debug!(
        "walk video files source={} root={} completed videos={}",
        source_id,
        root_path,
        videos.len()
    );
    Ok(ok(videos))
}

/// Collect all video files (convenience wrapper around the streaming version).
pub async fn walk_video_files(vfs: Arc<Vfs>, root_path: &str, source_id: &str) -> Result<Vec<VideoFileInfo>, String> {
    let (tx, mut rx) = mpsc::channel(256);
    let rp = root_path.to_owned();
    let sid = source_id.to_owned();

    let walk_handle = tokio::spawn(async move { walk_video_files_streaming(vfs, &rp, &sid, tx).await });

    let mut results = Vec::new();
    while let Some(video) = rx.recv().await {
        results.push(video);
    }

    walk_handle.await.map_err(|e| e.to_string())??;
    Ok(results)
}

/// Walk video files using concurrent BFS, streaming results through `tx`.
///
/// Each discovered video file is sent immediately — the caller can start
/// processing while the walk is still in progress.
pub async fn walk_video_files_streaming(
    vfs: Arc<Vfs>,
    root_path: &str,
    source_id: &str,
    tx: mpsc::Sender<VideoFileInfo>,
) -> Result<WalkStats, String> {
    let mut progress = WalkProgress {
        visited_dirs: 0,
        found_videos: 0,
    };
    let mut last_log = Instant::now();

    // BFS work queue of directories to visit
    let mut pending_dirs: VecDeque<PathBuf> = VecDeque::new();
    pending_dirs.push_back(PathBuf::from(root_path));

    // In-flight concurrent list operations
    let mut in_flight: FuturesUnordered<tokio::task::JoinHandle<Result<ListResult, String>>> = FuturesUnordered::new();

    // Seed initial tasks
    let initial_count = WALK_CONCURRENCY.min(pending_dirs.len());
    for _ in 0..initial_count {
        if let Some(dir) = pending_dirs.pop_front() {
            in_flight.push(spawn_list_dir(vfs.clone(), dir));
        }
    }

    while !in_flight.is_empty() {
        let join_result = in_flight.next().await.unwrap();
        let list_result = join_result.map_err(|e| e.to_string())??;

        progress.visited_dirs += 1;

        // Log progress every 2 seconds
        if last_log.elapsed().as_secs() >= 2 {
            debug!(
                "walk progress source={source_id} dirs={} videos={} current={}",
                progress.visited_dirs, progress.found_videos, list_result.dir_path
            );
            last_log = Instant::now();
        }

        // Process results
        for child_dir in list_result.child_dirs {
            pending_dirs.push_back(child_dir);
        }
        for video in list_result.videos {
            progress.found_videos += 1;
            if tx.send(video).await.is_err() {
                // Receiver dropped — stop walking
                return Ok(WalkStats {
                    visited_dirs: progress.visited_dirs,
                    found_videos: progress.found_videos,
                });
            }
        }

        // Fill up to WALK_CONCURRENCY in-flight tasks
        while in_flight.len() < WALK_CONCURRENCY {
            if let Some(dir) = pending_dirs.pop_front() {
                in_flight.push(spawn_list_dir(vfs.clone(), dir));
            } else {
                break;
            }
        }
    }

    debug!(
        "walk complete source={source_id} dirs={} videos={}",
        progress.visited_dirs, progress.found_videos
    );

    Ok(WalkStats {
        visited_dirs: progress.visited_dirs,
        found_videos: progress.found_videos,
    })
}

/// Walk files matching custom extensions using concurrent BFS, streaming results through `tx`.
///
/// Like `walk_video_files_streaming` but parameterized by file extensions.
/// Extensions should be lowercase with leading dot (e.g., ".jpg", ".png").
pub async fn walk_files_streaming(
    vfs: Arc<Vfs>,
    root_path: &str,
    source_id: &str,
    extensions: &'static [&'static str],
    tx: mpsc::Sender<VideoFileInfo>,
) -> Result<WalkStats, String> {
    let mut progress = WalkProgress {
        visited_dirs: 0,
        found_videos: 0,
    };
    let mut last_log = Instant::now();

    let mut pending_dirs: VecDeque<PathBuf> = VecDeque::new();
    pending_dirs.push_back(PathBuf::from(root_path));

    let mut in_flight: FuturesUnordered<tokio::task::JoinHandle<Result<ListResult, String>>> = FuturesUnordered::new();

    let initial_count = WALK_CONCURRENCY.min(pending_dirs.len());
    for _ in 0..initial_count {
        if let Some(dir) = pending_dirs.pop_front() {
            in_flight.push(spawn_list_dir_ext(vfs.clone(), dir, extensions));
        }
    }

    while !in_flight.is_empty() {
        let join_result = in_flight.next().await.unwrap();
        let list_result = join_result.map_err(|e| e.to_string())??;

        progress.visited_dirs += 1;

        if last_log.elapsed().as_secs() >= 2 {
            debug!(
                "walk progress source={source_id} dirs={} files={} current={}",
                progress.visited_dirs, progress.found_videos, list_result.dir_path
            );
            last_log = Instant::now();
        }

        for child_dir in list_result.child_dirs {
            pending_dirs.push_back(child_dir);
        }
        for file in list_result.videos {
            progress.found_videos += 1;
            if tx.send(file).await.is_err() {
                return Ok(WalkStats {
                    visited_dirs: progress.visited_dirs,
                    found_videos: progress.found_videos,
                });
            }
        }

        while in_flight.len() < WALK_CONCURRENCY {
            if let Some(dir) = pending_dirs.pop_front() {
                in_flight.push(spawn_list_dir_ext(vfs.clone(), dir, extensions));
            } else {
                break;
            }
        }
    }

    debug!(
        "walk complete source={source_id} dirs={} files={}",
        progress.visited_dirs, progress.found_videos
    );

    Ok(WalkStats {
        visited_dirs: progress.visited_dirs,
        found_videos: progress.found_videos,
    })
}

// ── per-directory listing ───────────────────────────────────────────────

struct ListResult {
    dir_path: String,
    child_dirs: Vec<PathBuf>,
    videos: Vec<VideoFileInfo>,
}

fn spawn_list_dir(vfs: Arc<Vfs>, dir: PathBuf) -> tokio::task::JoinHandle<Result<ListResult, String>> {
    tokio::spawn(async move { list_single_dir(&vfs, &dir).await })
}

fn spawn_list_dir_ext(
    vfs: Arc<Vfs>,
    dir: PathBuf,
    extensions: &'static [&'static str],
) -> tokio::task::JoinHandle<Result<ListResult, String>> {
    tokio::spawn(async move { list_single_dir_ext(&vfs, &dir, extensions).await })
}

async fn list_single_dir(vfs: &Vfs, dir: &StdPath) -> Result<ListResult, String> {
    let dir_display = dir.to_string_lossy().to_string();

    let entries = vfs.list(dir).await.map_err(|err| err.to_string())?;
    let visible_entries: Vec<_> = entries
        .into_iter()
        .filter(|entry| !entry.name.starts_with('.'))
        .collect();

    // BDMV detection — treat the whole directory as a single video
    if visible_entries
        .iter()
        .any(|entry| entry.is_dir && entry.name.eq_ignore_ascii_case("BDMV"))
    {
        let mut videos = Vec::new();
        if let Some(main) = pick_bdmv_main_file(vfs, dir).await? {
            videos.push(main);
        }
        return Ok(ListResult {
            dir_path: dir_display,
            child_dirs: Vec::new(),
            videos,
        });
    }

    let mut child_dirs = Vec::new();
    let mut videos = Vec::new();

    for entry in visible_entries {
        let full_path = normalize_source_path(&entry.path).map_err(|err| err.clone())?;
        if entry.is_dir {
            child_dirs.push(PathBuf::from(&full_path));
            continue;
        }

        let ext = StdPath::new(&entry.name)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{}", value.to_lowercase()))
            .unwrap_or_default();
        if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
            videos.push(VideoFileInfo {
                file_path: full_path,
                dir_path: normalize_source_path(&dir.to_string_lossy()).map_err(|err| err.clone())?,
                file_size: entry.size,
                mtime: entry.modified.map_or(0, |dt| dt.timestamp()),
            });
        }
    }

    Ok(ListResult {
        dir_path: dir_display,
        child_dirs,
        videos,
    })
}

/// Like `list_single_dir` but matches against a custom set of extensions.
async fn list_single_dir_ext(vfs: &Vfs, dir: &StdPath, extensions: &[&str]) -> Result<ListResult, String> {
    let dir_display = dir.to_string_lossy().to_string();
    let entries = vfs.list(dir).await.map_err(|err| err.to_string())?;
    let visible_entries: Vec<_> = entries
        .into_iter()
        .filter(|entry| !entry.name.starts_with('.'))
        .collect();

    let mut child_dirs = Vec::new();
    let mut videos = Vec::new();

    for entry in visible_entries {
        let full_path = normalize_source_path(&entry.path).map_err(|err| err.clone())?;
        if entry.is_dir {
            child_dirs.push(PathBuf::from(&full_path));
            continue;
        }

        let ext = StdPath::new(&entry.name)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{}", value.to_lowercase()))
            .unwrap_or_default();
        if extensions.contains(&ext.as_str()) {
            videos.push(VideoFileInfo {
                file_path: full_path,
                dir_path: normalize_source_path(&dir.to_string_lossy()).map_err(|err| err.clone())?,
                file_size: entry.size,
                mtime: entry.modified.map_or(0, |dt| dt.timestamp()),
            });
        }
    }

    Ok(ListResult {
        dir_path: dir_display,
        child_dirs,
        videos,
    })
}

// ── BDMV handling ───────────────────────────────────────────────────────

async fn pick_bdmv_main_file(vfs: &Vfs, bdmv_parent_dir: &StdPath) -> Result<Option<VideoFileInfo>, String> {
    let stream_dir = format!(
        "{}/BDMV/STREAM",
        normalize_source_path(&bdmv_parent_dir.to_string_lossy())
            .map_err(|err| err.clone())?
            .trim_end_matches('/')
    );
    let Ok(entries) = vfs.list(StdPath::new(&stream_dir)).await else {
        return Ok(None);
    };

    let best = entries
        .into_iter()
        .filter(|entry| !entry.is_dir)
        .filter(|entry| {
            StdPath::new(&entry.name)
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("m2ts"))
        })
        .max_by_key(|entry| entry.size);

    Ok(best.map(|entry| VideoFileInfo {
        file_path: normalize_source_path(&entry.path).unwrap_or_else(|_| entry.path.clone()),
        dir_path: normalize_source_path(&bdmv_parent_dir.to_string_lossy())
            .unwrap_or_else(|_| bdmv_parent_dir.to_string_lossy().into_owned()),
        file_size: entry.size,
        mtime: entry.modified.map_or(0, |dt| dt.timestamp()),
    }))
}
