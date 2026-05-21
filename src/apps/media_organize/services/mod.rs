//! Media organize service — in-memory session management for file organization.
//!
//! Mirrors the TS `MediaOrganizeService` class. The session state is held in
//! `AppState::organize_session` (`Arc<RwLock<Option<OrganizeSession>>>`).

use serde::{Deserialize, Serialize};

// ── Session types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeSession {
    pub id: String,
    pub status: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub items: Vec<OrganizeItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<OrganizeProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<OrganizeReport>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeItem {
    pub id: String,
    pub source_path: String,
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_dir: Option<String>,
    pub is_directory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<OrganizeItem>>,
    pub parsed: ParsedMediaInfo,
    pub tmdb_match: TmdbMatchResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_app_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    pub link_mode: String,
    pub item_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_disc: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adult_match: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music_match: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedMediaInfo {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episodes: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_group: Option<String>,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music_year: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TmdbMatchResult {
    pub status: String,
    pub candidates: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_detail: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeProgress {
    pub current: i64,
    pub total: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeReport {
    pub total_items: i64,
    pub success_count: i64,
    pub failed_count: i64,
    pub skipped_count: i64,
    pub results: Vec<OrganizeReportItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeReportItem {
    pub item_id: String,
    pub file_name: String,
    pub status: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfo_info: Option<serde_json::Value>,
}

// ── Tree helpers ──────────────────────────────────────────────────────────────

/// Find an item by ID in a tree (recursive).
pub fn find_item_by_id_mut<'a>(items: &'a mut [OrganizeItem], id: &str) -> Option<&'a mut OrganizeItem> {
    for item in items.iter_mut() {
        if item.id == id {
            return Some(item);
        }
        if let Some(ref mut children) = item.children
            && let Some(found) = find_item_by_id_mut(children, id)
        {
            return Some(found);
        }
    }
    None
}

/// Flatten all leaf items (files and disc entries) from the tree.
pub fn flatten_items(items: &[OrganizeItem]) -> Vec<&OrganizeItem> {
    let mut result = Vec::new();
    for item in items {
        if item.is_disc == Some(true) {
            result.push(item);
        } else if let Some(ref children) = item.children {
            if !children.is_empty() {
                result.extend(flatten_items(children));
            }
        } else if !item.is_directory {
            result.push(item);
        }
    }
    result
}

/// Flatten items mutably.
pub fn flatten_items_mut(items: &mut [OrganizeItem]) -> Vec<&mut OrganizeItem> {
    let mut result = Vec::new();
    for item in items.iter_mut() {
        if item.is_disc == Some(true) {
            result.push(item);
        } else if item.children.is_some() && !item.is_directory {
            // leaf file with empty children — shouldn't happen but handle
            result.push(item);
        } else if item.is_directory {
            if let Some(ref mut children) = item.children {
                result.extend(flatten_items_mut(children));
            }
        } else {
            result.push(item);
        }
    }
    result
}

// ── File type detection ───────────────────────────────────────────────────────

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "wmv", "flv", "mov", "m4v", "ts", "m2ts", "webm", "rmvb", "rm", "mpg", "mpeg", "vob", "iso",
    "3gp", "ogv",
];

const MUSIC_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "aac", "ogg", "wma", "m4a", "alac", "aiff", "ape", "opus", "dsf", "dff",
];

const DISC_FOLDER_NAMES: &[&str] = &["bdmv", "video_ts", "certificate"];

pub fn is_video_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    VIDEO_EXTENSIONS.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

pub fn is_music_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    MUSIC_EXTENSIONS.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

pub fn is_disc_folder(name: &str) -> bool {
    let lower = name.to_lowercase();
    DISC_FOLDER_NAMES.iter().any(|n| lower == *n)
}

/// Simple media filename parser — extracts title, year, content type.
pub fn parse_media_filename(name: &str) -> ParsedMediaInfo {
    // Strip extension
    let stem = if let Some(pos) = name.rfind('.') {
        if is_video_file(name) || is_music_file(name) {
            &name[..pos]
        } else {
            name
        }
    } else {
        name
    };

    // Try to extract year (4-digit number in parentheses or after dot/space)
    let year_re = regex::Regex::new(r"[\(\[\.\s](\d{4})[\)\]\.\s]").unwrap();
    let year = year_re
        .captures(stem)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .filter(|&y| (1900..=2099).contains(&y));

    // Extract title (everything before year or resolution markers)
    let title_end_re = regex::Regex::new(
        r"[\.\s\[\(](19|20)\d{2}|[\.\s](2160p|1080p|720p|480p|4K|UHD)|[\.\s](BluRay|WEB-DL|WEBRip|BDRip|HDRip|DVDRip|HDTV)"
    ).unwrap();
    let title = if let Some(m) = title_end_re.find(stem) {
        stem[..m.start()].trim()
    } else {
        stem.trim()
    };

    // Clean title: replace dots/underscores with spaces
    let title = title.replace(['.', '_'], " ").trim().to_string();

    // Detect season/episode
    let se_re = regex::Regex::new(r"[Ss](\d{1,2})[Ee](\d{1,3})").unwrap();
    let (season, episodes) = if let Some(caps) = se_re.captures(stem) {
        let s = caps.get(1).and_then(|m| m.as_str().parse::<i32>().ok());
        let e = caps.get(2).and_then(|m| m.as_str().parse::<i32>().ok());
        (s, e.map(|ep| vec![ep]))
    } else {
        (None, None)
    };

    let content_type = if season.is_some() || episodes.is_some() {
        "tv"
    } else {
        "unknown"
    };

    ParsedMediaInfo {
        title,
        year,
        season,
        episodes,
        quality: None,
        codec: None,
        source: None,
        audio_codec: None,
        release_group: None,
        content_type: content_type.to_string(),
        artist: None,
        album_artist: None,
        album: None,
        track_title: None,
        track_number: None,
        disc_number: None,
        genre: None,
        music_year: None,
    }
}
