use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const VIDEO_EXTENSIONS: [&str; 15] = [
    ".mkv", ".mp4", ".avi", ".ts", ".rmvb", ".flv", ".wmv", ".mov", ".m4v", ".mpg", ".mpeg", ".vob", ".m2ts", ".webm",
    ".iso",
];

pub const PHOTO_EXTENSIONS: [&str; 22] = [
    ".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tiff", ".tif", ".heic", ".heif", ".avif", ".raw", ".cr2",
    ".cr3", ".nef", ".arw", ".dng", ".orf", ".rw2", ".pef", ".srw", ".raf",
];

pub const BOOK_EXTENSIONS: [&str; 6] = [".txt", ".epub", ".mobi", ".azw3", ".pdf", ".cbz"];

pub const AUDIO_EXTENSIONS: [&str; 14] = [
    ".flac", ".mp3", ".m4a", ".ogg", ".opus", ".wav", ".aac", ".wma", ".ape", ".alac", ".dsf", ".dff", ".aiff", ".aif",
];

#[derive(Deserialize)]
pub struct PathQuery {
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatEntriesRequest {
    pub paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseBatchRequest {
    pub paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalkVideoFilesRequest {
    pub root_path: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BrowseEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    pub modified_at: Option<String>,
}

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BrowseDirectoryResponse {
    pub current_path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<BrowseEntry>,
}

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatEntry {
    pub path: String,
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    pub modified_at: Option<String>,
    pub mode: Option<String>,
}

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VideoFileInfo {
    pub file_path: String,
    pub dir_path: String,
    #[ts(type = "number")]
    pub file_size: u64,
    #[ts(type = "number")]
    pub mtime: i64,
}

pub struct WalkProgress {
    pub visited_dirs: usize,
    pub found_videos: usize,
}

/// Final statistics returned after a walk completes.
#[derive(Debug, Clone)]
pub struct WalkStats {
    pub visited_dirs: usize,
    pub found_videos: usize,
}
