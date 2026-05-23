//! Media organize handlers — session-based file organization.
//!
//! 18 endpoints total:
//! - Session: `get_session`, scan, `identify_item`, `identify_all`, `select_match`,
//!   `manual_search`, `manual_search_adult`, `select_adult_match`, `select_music_match`,
//!   `manual_search_music`, `reset_match`, `update_target`, execute, cancel, clear
//! - Reports: `list_reports`, `get_report`, `delete_report`
use crate::db::{ApiDateTimeExt, OptionalApiDateTimeExt};

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::fs as tfs;
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::AppState;
use crate::apps::media_organize::services::*;
use crate::db::repos::organize_report_repo::{CreateOrganizeReportInput, OrganizeReportRepo};
use crate::error::AppError;
use crate::error::OptionExt;
use crate::handlers::ok;

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanInput {
    pub path: String,
    pub source_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifyItemInput {
    pub item_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectMatchInput {
    pub item_id: String,
    pub tmdb_id: i64,
    pub media_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSearchInput {
    pub item_id: String,
    pub keyword: String,
    pub year: Option<i32>,
    pub media_type: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSearchAdultInput {
    pub item_id: String,
    pub keyword: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectAdultMatchInput {
    pub item_id: String,
    pub video_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectMusicMatchInput {
    pub item_id: String,
    pub mb_release_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSearchMusicInput {
    pub item_id: String,
    pub keyword: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetMatchInput {
    pub item_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTargetInput {
    pub item_id: String,
    pub folder_id: Option<String>,
    pub link_mode: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteInput {
    pub item_ids: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedResponse {
    pub started: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelledResponse {
    pub cancelled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearedResponse {
    pub cleared: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedResponse {
    pub deleted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummaryResponse {
    pub id: String,
    pub source_path: String,
    pub total_items: i64,
    pub success_count: i64,
    pub failed_count: i64,
    pub skipped_count: i64,
    pub media_names: Vec<String>,
    pub created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedReportResponse {
    pub id: String,
    pub source_path: String,
    pub total_items: i64,
    pub success_count: i64,
    pub failed_count: i64,
    pub skipped_count: i64,
    pub results: serde_json::Value,
    pub media_names: Vec<String>,
    pub created_at: String,
}

// ── Helper: load library settings (OrganizeContext analogue) ───────────────────

#[derive(Debug, Clone)]
struct OrganizeContext {
    id: String,
    #[allow(dead_code)]
    name: String,
    content_type: String,
    #[allow(dead_code)]
    target_path: String,
    link_mode: String,
    #[allow(dead_code)]
    folder_format: Option<String>,
    #[allow(dead_code)]
    file_format: Option<String>,
    organize_lang: Option<String>,
    flatten_disc: bool,
    #[allow(dead_code)]
    fix_emby_disc: bool,
    #[allow(dead_code)]
    strict_year_match: bool,
}

async fn load_app_contexts(db: &DatabaseConnection) -> Vec<OrganizeContext> {
    use crate::db::entities::{books, musics, videos};
    use crate::db::repos::media::VideoRepo;
    use sea_orm::*;

    let mut contexts = Vec::new();

    // Load video categories
    let video_libs = videos::Entity::find().all(db).await.unwrap_or_default();
    for lib in video_libs {
        let settings: serde_json::Value = lib.settings.clone().unwrap_or_else(|| serde_json::json!({}));
        let sources = VideoRepo::parse_sources(&lib.sources);
        let default_source = sources.iter().find(|s| s.2).or(sources.first());
        let target_path = default_source.map(|s| s.1.clone()).unwrap_or_default();

        contexts.push(OrganizeContext {
            id: lib.id.to_string(),
            name: lib.name.clone(),
            content_type: lib.r#type.clone(),
            target_path,
            link_mode: settings
                .get("linkMode")
                .and_then(|v| v.as_str())
                .unwrap_or("hardlink")
                .to_string(),
            folder_format: settings.get("folderFormat").and_then(|v| v.as_str()).map(String::from),
            file_format: settings.get("fileFormat").and_then(|v| v.as_str()).map(String::from),
            organize_lang: settings.get("organizeLang").and_then(|v| v.as_str()).map(String::from),
            flatten_disc: settings
                .get("flattenDisc")
                .and_then(sea_orm::JsonValue::as_bool)
                .unwrap_or(false),
            fix_emby_disc: settings
                .get("fixEmbyDisc")
                .and_then(sea_orm::JsonValue::as_bool)
                .unwrap_or(false),
            strict_year_match: settings
                .get("strictYearMatch")
                .and_then(sea_orm::JsonValue::as_bool)
                .unwrap_or(false),
        });
    }

    // Load music categories
    let music_libs = musics::Entity::find().all(db).await.unwrap_or_default();
    for lib in music_libs {
        let settings: serde_json::Value = lib.settings.clone().unwrap_or_else(|| serde_json::json!({}));
        let sources = VideoRepo::parse_sources(&lib.sources);
        let default_source = sources.iter().find(|s| s.2).or(sources.first());
        let target_path = default_source.map(|s| s.1.clone()).unwrap_or_default();

        contexts.push(OrganizeContext {
            id: lib.id.to_string(),
            name: lib.name.clone(),
            content_type: "music".to_string(),
            target_path,
            link_mode: settings
                .get("linkMode")
                .and_then(|v| v.as_str())
                .unwrap_or("hardlink")
                .to_string(),
            folder_format: settings.get("folderFormat").and_then(|v| v.as_str()).map(String::from),
            file_format: settings.get("fileFormat").and_then(|v| v.as_str()).map(String::from),
            organize_lang: settings.get("organizeLang").and_then(|v| v.as_str()).map(String::from),
            flatten_disc: settings
                .get("flattenDisc")
                .and_then(sea_orm::JsonValue::as_bool)
                .unwrap_or(false),
            fix_emby_disc: settings
                .get("fixEmbyDisc")
                .and_then(sea_orm::JsonValue::as_bool)
                .unwrap_or(false),
            strict_year_match: settings
                .get("strictYearMatch")
                .and_then(sea_orm::JsonValue::as_bool)
                .unwrap_or(false),
        });
    }

    // Load book categories
    let book_libs = books::Entity::find().all(db).await.unwrap_or_default();
    for lib in book_libs {
        let settings: serde_json::Value = lib.settings.clone().unwrap_or_else(|| serde_json::json!({}));
        let sources = VideoRepo::parse_sources(&lib.sources);
        let default_source = sources.iter().find(|s| s.2).or(sources.first());
        let target_path = default_source.map(|s| s.1.clone()).unwrap_or_default();

        contexts.push(OrganizeContext {
            id: lib.id.to_string(),
            name: lib.name.clone(),
            content_type: "book".to_string(),
            target_path,
            link_mode: settings
                .get("linkMode")
                .and_then(|v| v.as_str())
                .unwrap_or("hardlink")
                .to_string(),
            folder_format: settings.get("folderFormat").and_then(|v| v.as_str()).map(String::from),
            file_format: settings.get("fileFormat").and_then(|v| v.as_str()).map(String::from),
            organize_lang: settings.get("organizeLang").and_then(|v| v.as_str()).map(String::from),
            flatten_disc: settings
                .get("flattenDisc")
                .and_then(sea_orm::JsonValue::as_bool)
                .unwrap_or(false),
            fix_emby_disc: settings
                .get("fixEmbyDisc")
                .and_then(sea_orm::JsonValue::as_bool)
                .unwrap_or(false),
            strict_year_match: settings
                .get("strictYearMatch")
                .and_then(sea_orm::JsonValue::as_bool)
                .unwrap_or(false),
        });
    }

    contexts
}

fn guess_target_app(contexts: &[OrganizeContext], content_type: &str) -> Option<String> {
    contexts
        .iter()
        .find(|c| c.content_type == content_type)
        .or(contexts.first())
        .map(|c| c.id.clone())
}

fn get_folder_link_mode(contexts: &[OrganizeContext], content_type: &str) -> String {
    contexts
        .iter()
        .find(|c| c.content_type == content_type)
        .map_or_else(|| "hardlink".to_string(), |c| c.link_mode.clone())
}

fn any_folder_has_flatten_disc(contexts: &[OrganizeContext]) -> bool {
    contexts.iter().any(|c| c.flatten_disc)
}

fn get_organize_lang(contexts: &[OrganizeContext]) -> Option<String> {
    contexts.iter().find_map(|c| c.organize_lang.clone())
}

// ── Directory scanning ────────────────────────────────────────────────────────

async fn scan_directory(
    dir_path: &str,
    contexts: &[OrganizeContext],
    source_id: Option<&str>,
) -> Result<Vec<OrganizeItem>, AppError> {
    let entries = list_entries(dir_path, source_id).await?;

    // Check if top-level contains disc folders
    if any_folder_has_flatten_disc(contexts) {
        let has_disc = entries.iter().any(|e| e.is_dir && is_disc_folder(&e.name));
        if has_disc {
            let disc_item = create_disc_item(dir_path, &dir_name(dir_path), contexts);
            return Ok(vec![disc_item]);
        }
    }

    let mut items = Vec::new();
    for entry in &entries {
        if entry.is_dir {
            if any_folder_has_flatten_disc(contexts) {
                let sub = list_entries(&entry.path, source_id).await?;
                let has_disc = sub.iter().any(|e| e.is_dir && is_disc_folder(&e.name));
                if has_disc {
                    items.push(create_disc_item(&entry.path, &entry.name, contexts));
                    continue;
                }
            }
            let children = Box::pin(scan_directory_recursive(&entry.path, &entry.name, contexts, source_id)).await?;
            if !children.is_empty() {
                items.push(create_directory_item(&entry.path, &entry.name, children, contexts));
            }
        } else if is_video_file(&entry.name) {
            items.push(create_file_item(&entry.path, dir_path, contexts, source_id).await);
        } else if is_music_file(&entry.name) {
            items.push(create_music_file_item(&entry.path, dir_path, contexts));
        }
    }
    Ok(items)
}

async fn scan_directory_recursive(
    dir_path: &str,
    _dir_name: &str,
    contexts: &[OrganizeContext],
    source_id: Option<&str>,
) -> Result<Vec<OrganizeItem>, AppError> {
    let entries = list_entries(dir_path, source_id).await?;

    if any_folder_has_flatten_disc(contexts) {
        let has_disc = entries.iter().any(|e| e.is_dir && is_disc_folder(&e.name));
        if has_disc {
            return Ok(vec![]);
        }
    }

    let mut items = Vec::new();
    for entry in &entries {
        if entry.is_dir {
            if any_folder_has_flatten_disc(contexts) {
                let sub = list_entries(&entry.path, source_id).await?;
                let has_disc = sub.iter().any(|e| e.is_dir && is_disc_folder(&e.name));
                if has_disc {
                    items.push(create_disc_item(&entry.path, &entry.name, contexts));
                    continue;
                }
            }
            let children = Box::pin(scan_directory_recursive(&entry.path, &entry.name, contexts, source_id)).await?;
            if !children.is_empty() {
                items.push(create_directory_item(&entry.path, &entry.name, children, contexts));
            }
        } else if is_video_file(&entry.name) {
            items.push(create_file_item(&entry.path, dir_path, contexts, source_id).await);
        } else if is_music_file(&entry.name) {
            items.push(create_music_file_item(&entry.path, dir_path, contexts));
        }
    }
    Ok(items)
}

struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
}

async fn list_entries(dir_path: &str, _source_id: Option<&str>) -> Result<Vec<DirEntry>, AppError> {
    // For now, only support local filesystem scanning
    let mut entries = Vec::new();
    let mut rd = tfs::read_dir(dir_path)
        .await
        .map_err(|e| AppError::BadRequest(format!("Cannot read directory {dir_path}: {e}")))?;

    while let Ok(Some(entry)) = rd.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let meta = entry.metadata().await.ok();
        let is_dir = meta.as_ref().is_some_and(std::fs::Metadata::is_dir);
        let path = entry.path().to_string_lossy().to_string();
        entries.push(DirEntry { name, path, is_dir });
    }

    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        a.name.cmp(&b.name)
    });

    Ok(entries)
}

fn dir_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn create_directory_item(
    dir_path: &str,
    name: &str,
    children: Vec<OrganizeItem>,
    contexts: &[OrganizeContext],
) -> OrganizeItem {
    let parsed = parse_media_filename(name);
    let link_mode = get_folder_link_mode(contexts, &parsed.content_type);
    let target_app_id = guess_target_app(contexts, &parsed.content_type);

    OrganizeItem {
        id: Uuid::new_v4().to_string(),
        source_path: dir_path.to_string(),
        file_name: name.to_string(),
        parent_dir: None,
        is_directory: true,
        children: Some(children),
        parsed,
        tmdb_match: TmdbMatchResult {
            status: "unmatched".to_string(),
            candidates: vec![],
            selected_id: None,
            selected_detail: None,
        },
        target_app_id,
        target_path: None,
        link_mode,
        item_status: "pending".to_string(),
        error: None,
        file_size: None,
        is_disc: None,
        adult_match: None,
        music_match: None,
    }
}

fn create_disc_item(dir_path: &str, name: &str, contexts: &[OrganizeContext]) -> OrganizeItem {
    let parsed = parse_media_filename(name);
    let link_mode = get_folder_link_mode(contexts, &parsed.content_type);
    let target_app_id = guess_target_app(contexts, &parsed.content_type);

    OrganizeItem {
        id: Uuid::new_v4().to_string(),
        source_path: dir_path.to_string(),
        file_name: name.to_string(),
        parent_dir: None,
        is_directory: true,
        children: None,
        parsed,
        tmdb_match: TmdbMatchResult {
            status: "unmatched".to_string(),
            candidates: vec![],
            selected_id: None,
            selected_detail: None,
        },
        target_app_id,
        target_path: None,
        link_mode,
        item_status: "pending".to_string(),
        error: None,
        file_size: None,
        is_disc: Some(true),
        adult_match: None,
        music_match: None,
    }
}

async fn create_file_item(
    file_path: &str,
    parent_dir: &str,
    contexts: &[OrganizeContext],
    source_id: Option<&str>,
) -> OrganizeItem {
    let file_name = dir_name(file_path);
    let parent_name = dir_name(parent_dir);
    let parsed = parse_media_filename(&file_name);
    let link_mode = get_folder_link_mode(contexts, &parsed.content_type);
    let target_app_id = guess_target_app(contexts, &parsed.content_type);

    let file_size = if source_id.is_none() {
        tfs::metadata(file_path).await.ok().map(|m| m.len() as i64)
    } else {
        None
    };

    OrganizeItem {
        id: Uuid::new_v4().to_string(),
        source_path: file_path.to_string(),
        file_name,
        parent_dir: Some(parent_name),
        is_directory: false,
        children: None,
        parsed,
        tmdb_match: TmdbMatchResult {
            status: "unmatched".to_string(),
            candidates: vec![],
            selected_id: None,
            selected_detail: None,
        },
        target_app_id,
        target_path: None,
        link_mode,
        item_status: "pending".to_string(),
        error: None,
        file_size,
        is_disc: None,
        adult_match: None,
        music_match: None,
    }
}

fn create_music_file_item(file_path: &str, parent_dir: &str, contexts: &[OrganizeContext]) -> OrganizeItem {
    let file_name = dir_name(file_path);
    let parent_name = dir_name(parent_dir);
    let mut parsed = parse_media_filename(&file_name);
    parsed.content_type = "music".to_string();
    let link_mode = get_folder_link_mode(contexts, "music");
    let target_app_id = guess_target_app(contexts, "music");

    OrganizeItem {
        id: Uuid::new_v4().to_string(),
        source_path: file_path.to_string(),
        file_name,
        parent_dir: Some(parent_name),
        is_directory: false,
        children: None,
        parsed,
        tmdb_match: TmdbMatchResult {
            status: "unmatched".to_string(),
            candidates: vec![],
            selected_id: None,
            selected_detail: None,
        },
        target_app_id,
        target_path: None,
        link_mode,
        item_status: "pending".to_string(),
        error: None,
        file_size: None,
        is_disc: None,
        adult_match: None,
        music_match: Some(serde_json::json!({
            "status": "unmatched",
            "candidates": []
        })),
    }
}

// ── Session handlers ──────────────────────────────────────────────────────────

/// GET /api/media-organize/session
pub async fn get_session(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::handlers::ApiResponse<Option<OrganizeSession>>>, AppError> {
    let session = state.organize_session.read().await;
    Ok(ok(session.clone()))
}

/// POST /api/media-organize/scan
pub async fn scan(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ScanInput>,
) -> Result<Json<crate::handlers::ApiResponse<OrganizeSession>>, AppError> {
    let path = input.path.trim().to_string();
    if path.is_empty() {
        return Err(AppError::BadRequest("路径不能为空".into()));
    }

    // Check if busy
    {
        let session = state.organize_session.read().await;
        if let Some(ref s) = *session {
            let busy_statuses = ["scanning", "identifying", "executing"];
            if busy_statuses.contains(&s.status.as_str()) {
                return Err(AppError::Conflict(
                    "当前有任务正在执行，请等待完成或取消后再操作".into(),
                ));
            }
        }
    }

    // Load library contexts
    let contexts = load_app_contexts(&state.db).await;

    // Resolve path
    let resolved = if input.source_id.is_some() {
        path.clone()
    } else {
        let p = std::path::Path::new(&path);
        let resolved = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        };
        resolved.to_string_lossy().to_string()
    };

    // Check path exists (local only)
    if input.source_id.is_none() {
        let meta = tfs::metadata(&resolved)
            .await
            .map_err(|_| AppError::NotFound(format!("路径不存在: {resolved}")))?;

        if !meta.is_dir() && !meta.is_file() {
            return Err(AppError::BadRequest("路径不是文件或目录".into()));
        }
    }

    let now = Utc::now().to_api_datetime();
    let session_id = Uuid::new_v4().to_string();

    // Create initial session
    let new_session = OrganizeSession {
        id: session_id,
        status: "scanning".to_string(),
        source_path: resolved.clone(),
        source_id: input.source_id.clone(),
        items: vec![],
        progress: None,
        report: None,
        created_at: now.clone(),
        updated_at: now,
    };

    {
        let mut session = state.organize_session.write().await;
        *session = Some(new_session);
    }

    // Scan directory
    let items = scan_directory(&resolved, &contexts, input.source_id.as_deref()).await;

    let mut session = state.organize_session.write().await;
    if let Some(ref mut s) = *session {
        match items {
            Ok(items) => {
                s.items = items;
                s.status = "scanned".to_string();
                s.updated_at = Utc::now().to_api_datetime();
            }
            Err(e) => {
                s.status = "scanned".to_string();
                s.updated_at = Utc::now().to_api_datetime();
                warn!("scan error: {e}");
            }
        }
        Ok(ok(s.clone()))
    } else {
        Err(AppError::Internal("Session lost during scan".into()))
    }
}

/// POST /api/media-organize/identify/{itemId}
pub async fn identify_item(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<String>,
) -> Result<Json<crate::handlers::ApiResponse<OrganizeItem>>, AppError> {
    let mut session = state.organize_session.write().await;
    let s = session.as_mut().not_found("无活跃会话")?;

    let item = find_item_by_id_mut(&mut s.items, &item_id).not_found("条目不存在")?;

    // For now, mark as identified (TMDB integration will be added later)
    item.item_status = "identified".to_string();
    s.updated_at = Utc::now().to_api_datetime();

    Ok(ok(item.clone()))
}

/// POST /api/media-organize/identify-all
pub async fn identify_all(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::handlers::ApiResponse<StartedResponse>>, AppError> {
    {
        let session = state.organize_session.read().await;
        let s = session.as_ref().not_found("无活跃会话")?;

        let busy_statuses = ["identifying", "executing"];
        if busy_statuses.contains(&s.status.as_str()) {
            return Err(AppError::Conflict("当前有任务正在执行".into()));
        }
    }

    // Set status to identifying
    {
        let mut session = state.organize_session.write().await;
        if let Some(ref mut s) = *session {
            s.status = "identifying".to_string();
            s.updated_at = Utc::now().to_api_datetime();
        }
    }

    // Spawn background identification
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        // Simple: mark all items as identified
        let mut session = state_clone.organize_session.write().await;
        if let Some(ref mut s) = *session {
            let items_to_identify: Vec<String> = flatten_items(&s.items)
                .iter()
                .filter(|i| i.item_status == "pending")
                .map(|i| i.id.clone())
                .collect();

            let total = items_to_identify.len() as i64;
            s.progress = Some(OrganizeProgress {
                current: 0,
                total,
                current_file: None,
            });

            for (idx, id) in items_to_identify.iter().enumerate() {
                if let Some(item) = find_item_by_id_mut(&mut s.items, id) {
                    item.item_status = "identified".to_string();
                }
                s.progress = Some(OrganizeProgress {
                    current: (idx + 1) as i64,
                    total,
                    current_file: None,
                });
            }

            s.status = "identified".to_string();
            s.progress = Some(OrganizeProgress {
                current: total,
                total,
                current_file: None,
            });
            s.updated_at = Utc::now().to_api_datetime();
        }
    });

    Ok(ok(StartedResponse { started: true }))
}

/// POST /api/media-organize/select-match
pub async fn select_match(
    State(state): State<Arc<AppState>>,
    Json(input): Json<SelectMatchInput>,
) -> Result<Json<crate::handlers::ApiResponse<OrganizeItem>>, AppError> {
    let mut session = state.organize_session.write().await;
    let s = session.as_mut().not_found("无活跃会话")?;

    let item = find_item_by_id_mut(&mut s.items, &input.item_id).not_found("条目不存在")?;

    item.tmdb_match.selected_id = Some(input.tmdb_id);
    item.tmdb_match.status = "matched".to_string();
    item.parsed.content_type = if input.media_type == "tv" {
        "tv".to_string()
    } else {
        "movie".to_string()
    };
    item.item_status = "ready".to_string();
    s.updated_at = Utc::now().to_api_datetime();

    Ok(ok(item.clone()))
}

/// POST /api/media-organize/manual-search
pub async fn manual_search(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ManualSearchInput>,
) -> Result<Json<crate::handlers::ApiResponse<Vec<serde_json::Value>>>, AppError> {
    let session = state.organize_session.read().await;
    let _s = session.as_ref().not_found("无活跃会话")?;

    // Search TMDB using the Rust client
    let contexts = load_app_contexts(&state.db).await;
    let lang = get_organize_lang(&contexts).unwrap_or_else(|| "zh-CN".to_string());

    let tmdb_settings = {
        use crate::config::TmdbSettings;
        use crate::db::repos::system_config_repo::SystemConfigRepo;
        SystemConfigRepo::get::<TmdbSettings>(&state.db).await.ok()
    };

    let api_key = tmdb_settings
        .as_ref()
        .and_then(|s| s.api_key.clone())
        .or_else(|| std::env::var("TMDB_API_KEY").ok());

    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => {
            return Err(AppError::BadRequest(
                "TMDB API Key 未配置，请在设置中配置 TMDB API Key".into(),
            ));
        }
    };

    let tmdb = tokimo_media_scraper::metadata_providers::tmdb::TmdbClient::new(
        tokimo_media_scraper::metadata_providers::tmdb::TmdbConfig {
            api_key,
            language: Some(lang),
            base_url: None,
            image_base_url: None,
            cache_ttl: None,
            http_client: state.http_client.clone(),
        },
    );

    let media_type = input.media_type.as_deref().unwrap_or("movie");
    let year_param = input.year.map(|y| y as u32);
    let results = if media_type == "tv" {
        tmdb.search_tv(&input.keyword, year_param, 1).await
    } else {
        tmdb.search_movies(&input.keyword, year_param, 1).await
    };

    match results {
        Ok(items) => {
            let values: Vec<serde_json::Value> = items
                .into_iter()
                .map(|item| serde_json::to_value(item).unwrap_or_default())
                .collect();

            // Update item candidates
            let mut session = state.organize_session.write().await;
            if let Some(ref mut s) = *session {
                if let Some(item) = find_item_by_id_mut(&mut s.items, &input.item_id) {
                    item.tmdb_match.status = if values.is_empty() {
                        "failed".to_string()
                    } else {
                        "multiple".to_string()
                    };
                    values.clone_into(&mut item.tmdb_match.candidates);
                }
                s.updated_at = Utc::now().to_api_datetime();
            }

            Ok(ok(values))
        }
        Err(e) => Err(AppError::BadRequest(format!("TMDB search failed: {e}"))),
    }
}

/// POST /api/media-organize/manual-search-adult
pub async fn manual_search_adult(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ManualSearchAdultInput>,
) -> Result<Json<crate::handlers::ApiResponse<Option<serde_json::Value>>>, AppError> {
    let session = state.organize_session.read().await;
    let _s = session.as_ref().not_found("无活跃会话")?;

    // Adult metadata search placeholder — returns null for now
    // Real implementation would call JavBus/JavDB/StashDB/TPDB APIs
    debug!("manual_search_adult: keyword={}", input.keyword);
    Ok(ok(None))
}

/// POST /api/media-organize/select-adult-match
pub async fn select_adult_match(
    State(state): State<Arc<AppState>>,
    Json(input): Json<SelectAdultMatchInput>,
) -> Result<Json<crate::handlers::ApiResponse<OrganizeItem>>, AppError> {
    let mut session = state.organize_session.write().await;
    let s = session.as_mut().not_found("无活跃会话")?;

    let item = find_item_by_id_mut(&mut s.items, &input.item_id).not_found("条目不存在")?;

    item.parsed.content_type = "adult".to_string();
    input.video_id.clone_into(&mut item.parsed.title);
    item.tmdb_match.status = "matched".to_string();
    item.item_status = "ready".to_string();
    s.updated_at = Utc::now().to_api_datetime();

    Ok(ok(item.clone()))
}

/// POST /api/media-organize/select-music-match
pub async fn select_music_match(
    State(state): State<Arc<AppState>>,
    Json(input): Json<SelectMusicMatchInput>,
) -> Result<Json<crate::handlers::ApiResponse<OrganizeItem>>, AppError> {
    let mut session = state.organize_session.write().await;
    let s = session.as_mut().not_found("无活跃会话")?;

    let item = find_item_by_id_mut(&mut s.items, &input.item_id).not_found("条目不存在")?;

    item.parsed.content_type = "music".to_string();
    if let Some(ref mut mm) = item.music_match
        && let Some(obj) = mm.as_object_mut()
    {
        obj.insert("status".to_string(), serde_json::json!("matched"));
    }
    item.item_status = "ready".to_string();
    s.updated_at = Utc::now().to_api_datetime();

    Ok(ok(item.clone()))
}

/// POST /api/media-organize/manual-search-music
pub async fn manual_search_music(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ManualSearchMusicInput>,
) -> Result<Json<crate::handlers::ApiResponse<Vec<serde_json::Value>>>, AppError> {
    use tokimo_media_scraper::metadata_providers::musicbrainz::MusicBrainzClient;

    // Validate session exists (read lock dropped immediately after check)
    state.organize_session.read().await.as_ref().not_found("无活跃会话")?;

    let mb = MusicBrainzClient::new();
    match mb.search_release_by_keyword(&input.keyword, 25).await {
        Ok(results) => {
            let values: Vec<serde_json::Value> = results
                .into_iter()
                .map(|r| serde_json::to_value(r).unwrap_or_default())
                .collect();

            let mut session = state.organize_session.write().await;
            if let Some(ref mut s) = *session {
                if let Some(item) = find_item_by_id_mut(&mut s.items, &input.item_id) {
                    item.music_match = Some(serde_json::json!({
                        "status": if values.is_empty() { "failed" } else { "multiple" },
                        "candidates": values,
                    }));
                }
                s.updated_at = Utc::now().to_api_datetime();
            }

            Ok(ok(values))
        }
        Err(e) => Err(AppError::BadRequest(format!("MusicBrainz search failed: {e}"))),
    }
}

/// POST /api/media-organize/reset-match
pub async fn reset_match(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ResetMatchInput>,
) -> Result<Json<crate::handlers::ApiResponse<OrganizeItem>>, AppError> {
    let mut session = state.organize_session.write().await;
    let s = session.as_mut().not_found("无活跃会话")?;

    let item = find_item_by_id_mut(&mut s.items, &input.item_id).not_found("条目不存在")?;

    item.tmdb_match = TmdbMatchResult {
        status: "unmatched".to_string(),
        candidates: vec![],
        selected_id: None,
        selected_detail: None,
    };
    item.adult_match = None;
    if item.parsed.content_type == "music" {
        item.music_match = Some(serde_json::json!({
            "status": "unmatched",
            "candidates": []
        }));
    } else {
        item.music_match = None;
    }
    item.target_path = None;
    item.item_status = "identified".to_string();
    s.updated_at = Utc::now().to_api_datetime();

    Ok(ok(item.clone()))
}

/// POST /api/media-organize/update-target
pub async fn update_target(
    State(state): State<Arc<AppState>>,
    Json(input): Json<UpdateTargetInput>,
) -> Result<Json<crate::handlers::ApiResponse<OrganizeItem>>, AppError> {
    let mut session = state.organize_session.write().await;
    let s = session.as_mut().not_found("无活跃会话")?;

    let item = find_item_by_id_mut(&mut s.items, &input.item_id).not_found("条目不存在")?;

    // For directory items, propagate to children
    if item.is_directory {
        if let Some(ref mut children) = item.children {
            let leaves = flatten_items_mut(children);
            for leaf in leaves {
                if leaf.item_status == "organizing" || leaf.item_status == "success" || leaf.item_status == "organized"
                {
                    continue;
                }
                if let Some(ref folder_id) = input.folder_id {
                    leaf.target_app_id = Some(folder_id.clone());
                }
                if let Some(ref lm) = input.link_mode {
                    lm.clone_into(&mut leaf.link_mode);
                }
            }
        }
    } else {
        if let Some(ref folder_id) = input.folder_id {
            item.target_app_id = Some(folder_id.clone());
        }
        if let Some(ref lm) = input.link_mode {
            lm.clone_into(&mut item.link_mode);
        }
    }

    s.updated_at = Utc::now().to_api_datetime();
    Ok(ok(item.clone()))
}

/// POST /api/media-organize/execute
pub async fn execute(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ExecuteInput>,
) -> Result<Json<crate::handlers::ApiResponse<StartedResponse>>, AppError> {
    {
        let session = state.organize_session.read().await;
        let s = session.as_ref().not_found("无活跃会话")?;

        if s.status == "executing" || s.status == "identifying" {
            return Err(AppError::Conflict("当前有任务正在执行".into()));
        }
    }

    // Set status to executing
    {
        let mut session = state.organize_session.write().await;
        if let Some(ref mut s) = *session {
            s.status = "executing".to_string();
            s.updated_at = Utc::now().to_api_datetime();
        }
    }

    let state_clone = Arc::clone(&state);
    let item_ids = input.item_ids;

    tokio::spawn(async move {
        run_execute(&state_clone, item_ids).await;
    });

    Ok(ok(StartedResponse { started: true }))
}

async fn run_execute(state: &Arc<AppState>, item_ids: Option<Vec<String>>) {
    let mut session = state.organize_session.write().await;
    let Some(s) = session.as_mut() else { return };

    let all_items = flatten_items(&s.items);
    let items_to_organize: Vec<String> = if let Some(ref ids) = item_ids {
        all_items
            .iter()
            .filter(|i| ids.contains(&i.id) && i.item_status == "ready")
            .map(|i| i.id.clone())
            .collect()
    } else {
        all_items
            .iter()
            .filter(|i| i.item_status == "ready")
            .map(|i| i.id.clone())
            .collect()
    };

    let total = items_to_organize.len() as i64;
    s.progress = Some(OrganizeProgress {
        current: 0,
        total,
        current_file: None,
    });

    let mut report_items = Vec::new();
    let mut completed = 0i64;

    for id in &items_to_organize {
        if let Some(item) = find_item_by_id_mut(&mut s.items, id) {
            item.item_status = "organizing".to_string();

            // Execute file operation
            let result = execute_single_item(item).await;
            match result {
                Ok(target_path) => {
                    item.item_status = "success".to_string();
                    report_items.push(OrganizeReportItem {
                        item_id: item.id.clone(),
                        file_name: item.file_name.clone(),
                        status: "success".to_string(),
                        source_path: item.source_path.clone(),
                        target_path: Some(target_path),
                        link_mode: Some(item.link_mode.clone()),
                        error: None,
                        nfo_info: None,
                    });
                }
                Err(e) => {
                    item.item_status = "failed".to_string();
                    item.error = Some(e.clone());
                    report_items.push(OrganizeReportItem {
                        item_id: item.id.clone(),
                        file_name: item.file_name.clone(),
                        status: "failed".to_string(),
                        source_path: item.source_path.clone(),
                        target_path: item.target_path.clone(),
                        link_mode: Some(item.link_mode.clone()),
                        error: Some(e.clone()),
                        nfo_info: None,
                    });
                }
            }

            completed += 1;
            s.progress = Some(OrganizeProgress {
                current: completed,
                total,
                current_file: Some(item.file_name.clone()),
            });
        }
    }

    // Build report
    let success_count = report_items.iter().filter(|r| r.status == "success").count() as i64;
    let failed_count = report_items.iter().filter(|r| r.status == "failed").count() as i64;
    let skipped_count = report_items.iter().filter(|r| r.status == "skipped").count() as i64;

    s.report = Some(OrganizeReport {
        total_items: report_items.len() as i64,
        success_count,
        failed_count,
        skipped_count,
        results: report_items,
    });
    s.status = "done".to_string();
    s.progress = Some(OrganizeProgress {
        current: total,
        total,
        current_file: None,
    });
    s.updated_at = Utc::now().to_api_datetime();

    // Save report to DB
    if let Some(ref report) = s.report {
        let source_path = s.source_path.clone();
        let results_json = serde_json::to_value(&report.results).unwrap_or_default();
        let media_names = collect_media_names(&s.items);
        let media_names_json = serde_json::to_value(&media_names).unwrap_or_default();

        if let Err(e) = OrganizeReportRepo::create(
            &state.db,
            CreateOrganizeReportInput {
                source_path: source_path.clone(),
                total_items: report.total_items.to_string(),
                success_count: report.success_count.to_string(),
                failed_count: report.failed_count.to_string(),
                skipped_count: report.skipped_count.to_string(),
                results: results_json,
                media_names: media_names_json,
            },
        )
        .await
        {
            error!("Failed to save organize report: {e}");
        }
    }
}

/// Execute a single file operation (hardlink/symlink/copy/move).
async fn execute_single_item(item: &OrganizeItem) -> Result<String, String> {
    let target = item.target_path.as_ref().ok_or("No target path set")?;

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(target).parent() {
        tfs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {e}"))?;
    }

    match item.link_mode.as_str() {
        "hardlink" => {
            tfs::hard_link(&item.source_path, target)
                .await
                .map_err(|e| format!("Hardlink failed: {e}"))?;
        }
        "softlink" => {
            #[cfg(unix)]
            {
                tfs::symlink(&item.source_path, target)
                    .await
                    .map_err(|e| format!("Symlink failed: {e}"))?;
            }
            #[cfg(windows)]
            {
                // On Windows, symlink creation requires distinguishing file vs directory.
                let is_dir = tfs::metadata(&item.source_path).await.is_ok_and(|m| m.is_dir());
                if is_dir {
                    tfs::symlink_dir(&item.source_path, target)
                        .await
                        .map_err(|e| format!("Symlink failed: {e}"))?;
                } else {
                    tfs::symlink_file(&item.source_path, target)
                        .await
                        .map_err(|e| format!("Symlink failed: {e}"))?;
                }
            }
        }
        "copy" => {
            tfs::copy(&item.source_path, target)
                .await
                .map_err(|e| format!("Copy failed: {e}"))?;
        }
        "move" => {
            tfs::rename(&item.source_path, target)
                .await
                .map_err(|e| format!("Move failed: {e}"))?;
        }
        other => {
            return Err(format!("Unknown link mode: {other}"));
        }
    }

    Ok(target.clone())
}

fn collect_media_names(items: &[OrganizeItem]) -> Vec<String> {
    let mut names = std::collections::HashSet::new();
    fn walk(items: &[OrganizeItem], names: &mut std::collections::HashSet<String>) {
        for item in items {
            if let Some(ref detail) = item.tmdb_match.selected_detail {
                if let Some(title) = detail.get("title").and_then(|v| v.as_str()) {
                    let year = detail
                        .get("releaseDate")
                        .and_then(|v| v.as_str())
                        .map_or("", |d| &d[..4.min(d.len())]);
                    if year.is_empty() {
                        names.insert(title.to_string());
                    } else {
                        names.insert(format!("{title} ({year})"));
                    }
                }
            } else if !item.parsed.title.is_empty() && !item.is_directory {
                let t = &item.parsed.title;
                if let Some(y) = item.parsed.year {
                    names.insert(format!("{t} ({y})"));
                } else {
                    names.insert(t.clone());
                }
            }
            if let Some(ref children) = item.children {
                walk(children, names);
            }
        }
    }
    walk(items, &mut names);
    names.into_iter().collect()
}

/// POST /api/media-organize/cancel
pub async fn cancel(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::handlers::ApiResponse<CancelledResponse>>, AppError> {
    let mut session = state.organize_session.write().await;
    if let Some(ref mut s) = *session {
        let busy = s.status == "identifying" || s.status == "executing";
        if !busy {
            return Err(AppError::BadRequest("当前没有正在执行的任务".into()));
        }
        // Reset to previous terminal state
        s.status = if s.status == "identifying" {
            "scanned".to_string()
        } else {
            "done".to_string()
        };
        s.updated_at = Utc::now().to_api_datetime();
    } else {
        return Err(AppError::NotFound("无活跃会话".into()));
    }

    Ok(ok(CancelledResponse { cancelled: true }))
}

/// POST /api/media-organize/clear
pub async fn clear(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::handlers::ApiResponse<ClearedResponse>>, AppError> {
    let mut session = state.organize_session.write().await;
    if let Some(ref s) = *session {
        let busy = s.status == "identifying" || s.status == "executing" || s.status == "scanning";
        if busy {
            return Err(AppError::Conflict("请先取消正在执行的任务".into()));
        }
    }
    *session = None;
    Ok(ok(ClearedResponse { cleared: true }))
}

// ── Report handlers ───────────────────────────────────────────────────────────

/// GET /api/media-organize/reports
pub async fn list_reports(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::handlers::ApiResponse<Vec<ReportSummaryResponse>>>, AppError> {
    let rows = OrganizeReportRepo::list(&state.db).await?;
    let summaries: Vec<ReportSummaryResponse> = rows
        .into_iter()
        .map(|r| {
            let media_names: Vec<String> = serde_json::from_value(r.media_names).unwrap_or_default();
            ReportSummaryResponse {
                id: r.id.to_string(),
                source_path: r.source_path,
                total_items: r.total_items.parse().unwrap_or(0),
                success_count: r.success_count.parse().unwrap_or(0),
                failed_count: r.failed_count.parse().unwrap_or(0),
                skipped_count: r.skipped_count.parse().unwrap_or(0),
                media_names,
                created_at: r.created_at.to_api_datetime_or_default(),
            }
        })
        .collect();
    Ok(ok(summaries))
}

/// GET /api/media-organize/reports/{id}
pub async fn get_report(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<crate::handlers::ApiResponse<SavedReportResponse>>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::BadRequest("Invalid report ID".into()))?;
    let row = OrganizeReportRepo::get_by_id(&state.db, uuid)
        .await?
        .not_found("报告不存在")?;

    let media_names: Vec<String> = serde_json::from_value(row.media_names).unwrap_or_default();

    Ok(ok(SavedReportResponse {
        id: row.id.to_string(),
        source_path: row.source_path,
        total_items: row.total_items.parse().unwrap_or(0),
        success_count: row.success_count.parse().unwrap_or(0),
        failed_count: row.failed_count.parse().unwrap_or(0),
        skipped_count: row.skipped_count.parse().unwrap_or(0),
        results: row.results,
        media_names,
        created_at: row.created_at.to_api_datetime_or_default(),
    }))
}

/// DELETE /api/media-organize/reports/{id}
pub async fn delete_report(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<crate::handlers::ApiResponse<DeletedResponse>>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::BadRequest("Invalid report ID".into()))?;
    OrganizeReportRepo::delete(&state.db, uuid).await?;
    Ok(ok(DeletedResponse { deleted: true }))
}
