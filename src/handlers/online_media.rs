use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use rust_online_media_ingest::{
    models::{
        AnalyzeOnlineMediaRequest, AnalyzeOnlineMediaResponse, BatchCreateTasksRequest, BatchCreateTasksResponse,
        CancelTaskResponse, CreateTaskRequest, CreateTaskResponse, HealthResponse, ResolveCollectionRequest,
        ResolveCollectionResponse, TaskStatusResponse,
    },
    provider_catalog,
    providers::{analyze_url, resolve_collection_url},
    runtime::spawn_task,
};
use sea_orm::{DatabaseConnection, TransactionTrait};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use tracing::info;
use ts_rs::TS;
use uuid::Uuid;

use crate::{
    AppState,
    bus_clients::downloader::{CreateRecordRequest, UpdateDownloaderStatusRequest},
    db::repos::{
        download_record_repo::{CreateDownloadRecordInput, DownloadRecordRepo},
        job_repo::JobRepo,
        media::VideoRepo,
        ytdlp_provider_auth_repo::YtdlpProviderAuthRepo,
    },
    error::{AppError, OptionExt},
    handlers::{ApiResponse, ok, user::AuthUser},
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

async fn push_downloader_status(state: &Arc<AppState>, request: UpdateDownloaderStatusRequest) {
    let Some(client) = state.bus_client.get() else { return };
    if let Err(error) = crate::bus_clients::downloader::update_status(client, &request).await {
        tracing::error!(%error, record_id = %request.record_id, "failed to push downloader status to host");
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// DTOs
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderListEntry {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub source_site: String,
    pub supported_content_types: Vec<String>,
    pub requires_auth: bool,
    pub auth_configurable: bool,
    pub common_source_sites: Vec<String>,
    pub source_site_aliases: Vec<String>,
    pub host_suffixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderListEntry>,
    pub ytdlp_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineMediaAuthData {
    pub display_name: String,
    pub cookie: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AuthSettingResponse {
    pub provider_id: String,
    pub display_name: String,
    pub requires_auth: bool,
    pub cookie: Option<String>,
    pub is_enabled: bool,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateAuthSettingRequest {
    pub display_name: Option<String>,
    pub cookie: Option<String>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartOnlineMediaDownloadInput {
    pub url: String,
    pub target_app_id: String,
    pub media_title: Option<String>,
    pub media_year: Option<String>,
    #[serde(default = "default_true")]
    pub auto_organize: bool,
    #[serde(default)]
    pub confirm_duplicate: bool,
    pub existing_record_id: Option<String>,
    #[serde(default = "default_auto")]
    pub download_format: String,
    pub analysis: JsonValue,
}

fn default_true() -> bool {
    true
}

fn default_auto() -> String {
    "auto".into()
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartDownloadSuccessOutput {
    pub action: String,
    pub record_id: String,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartDownloadDuplicateOutput {
    pub action: String,
    pub existing_record_id: String,
    pub existing_status: String,
    pub existing_title: Option<String>,
    pub existing_source_site: Option<String>,
    pub existing_source_url: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum StartDownloadOutput {
    Success(StartDownloadSuccessOutput),
    Duplicate(StartDownloadDuplicateOutput),
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RetryDownloadInput {
    pub record_id: String,
}

struct TargetLib {
    id: Uuid,
    r#type: String,
}

fn metadata_string(metadata: Option<&JsonValue>, key: &str) -> Option<String> {
    metadata
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .map(String::from)
}

// ──────────────────────────────────────────────────────────────────────────────
// Ingest handlers
// ──────────────────────────────────────────────────────────────────────────────

pub async fn health() -> Json<ApiResponse<HealthResponse>> {
    ok(HealthResponse { ok: true })
}

pub async fn analyze(
    State(_state): State<Arc<AppState>>,
    Json(input): Json<AnalyzeOnlineMediaRequest>,
) -> Json<ApiResponse<AnalyzeOnlineMediaResponse>> {
    ok(analyze_url(&input).await)
}

pub async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateTaskRequest>,
) -> Json<ApiResponse<CreateTaskResponse>> {
    let task_id = state.online_media.tasks.create_task(input.clone()).await;
    spawn_task((*state.online_media).clone(), task_id.clone(), input);
    ok(CreateTaskResponse { task_id })
}

pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<TaskStatusResponse>>, AppError> {
    let task = state
        .online_media
        .tasks
        .get_task(&task_id)
        .await
        .not_found(format!("task not found: {task_id}"))?;
    Ok(ok(task.to_response()))
}

pub async fn cancel_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Json<ApiResponse<CancelTaskResponse>> {
    ok(CancelTaskResponse {
        success: state.online_media.tasks.request_cancel(&task_id).await,
    })
}

pub async fn resolve_collection(
    State(_state): State<Arc<AppState>>,
    Json(input): Json<ResolveCollectionRequest>,
) -> Result<Json<ApiResponse<ResolveCollectionResponse>>, AppError> {
    let result = resolve_collection_url(&input).await.map_err(AppError::BadRequest)?;
    Ok(ok(result))
}

pub async fn batch_create_tasks(
    State(state): State<Arc<AppState>>,
    Json(input): Json<BatchCreateTasksRequest>,
) -> Json<ApiResponse<BatchCreateTasksResponse>> {
    let mut task_ids = Vec::with_capacity(input.items.len());
    for item in &input.items {
        let req = CreateTaskRequest {
            record_id: item.record_id.clone(),
            url: item.url.clone(),
            normalized_url: None,
            provider_id: item.provider_id.clone(),
            auth: input.auth.clone(),
            audio_only: item.audio_only,
            audio_container: item.audio_container.clone(),
            target_library_id: input.target_library_id.clone(),
            target_folder_config_snapshot: input.target_folder_config_snapshot.clone(),
            metadata: item.metadata.clone(),
        };
        let task_id = state.online_media.tasks.create_task(req.clone()).await;
        spawn_task((*state.online_media).clone(), task_id.clone(), req);
        task_ids.push(task_id);
    }
    let total = task_ids.len();
    ok(BatchCreateTasksResponse { task_ids, total })
}

// ──────────────────────────────────────────────────────────────────────────────
// Providers/auth handlers
// ──────────────────────────────────────────────────────────────────────────────

pub async fn list_providers() -> Result<Json<ApiResponse<ProvidersResponse>>, AppError> {
    let catalog_response = provider_catalog::list_all_providers_with_ytdlp().await;

    let providers = catalog_response
        .providers
        .into_iter()
        .map(|p| ProviderListEntry {
            id: p.id,
            name: p.name,
            display_name: p.display_name,
            source_site: p.source_site,
            supported_content_types: p.supported_content_types,
            requires_auth: p.requires_auth,
            auth_configurable: p.auth_configurable,
            common_source_sites: p.common_source_sites,
            source_site_aliases: p.source_site_aliases,
            host_suffixes: p.host_suffixes,
        })
        .collect();

    Ok(ok(ProvidersResponse {
        providers,
        ytdlp_available: catalog_response.ytdlp_available,
    }))
}

pub async fn get_auth_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<AuthSettingResponse>>>, AppError> {
    let db = &state.db;

    let all_providers = provider_catalog::list_all_providers();
    let auth_configurable_providers: Vec<_> = all_providers.iter().filter(|p| p.auth_configurable).collect();

    let stored_settings = YtdlpProviderAuthRepo::get_all(db).await?;
    let stored_map: std::collections::HashMap<String, _> =
        stored_settings.into_iter().map(|s| (s.provider.clone(), s)).collect();

    let mut results = Vec::new();
    for provider in auth_configurable_providers {
        let response = if let Some(stored) = stored_map.get(&provider.id) {
            let auth_data: OnlineMediaAuthData =
                serde_json::from_value(stored.value.clone()).unwrap_or_else(|_| OnlineMediaAuthData {
                    display_name: provider.display_name.clone(),
                    cookie: None,
                    is_enabled: true,
                });

            AuthSettingResponse {
                provider_id: provider.id.clone(),
                display_name: auth_data.display_name,
                requires_auth: provider.requires_auth,
                cookie: auth_data.cookie,
                is_enabled: auth_data.is_enabled,
                updated_at: Some(stored.updated_at.to_rfc3339()),
            }
        } else {
            AuthSettingResponse {
                provider_id: provider.id.clone(),
                display_name: provider.display_name.clone(),
                requires_auth: provider.requires_auth,
                cookie: None,
                is_enabled: true,
                updated_at: None,
            }
        };
        results.push(response);
    }

    Ok(ok(results))
}

pub async fn update_auth_setting(
    State(state): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    Json(req): Json<UpdateAuthSettingRequest>,
) -> Result<Json<ApiResponse<AuthSettingResponse>>, AppError> {
    let db = &state.db;

    let all_providers = provider_catalog::list_all_providers();
    let provider = all_providers
        .iter()
        .find(|p| p.id == provider_id)
        .not_found(format!("provider not found: {provider_id}"))?;

    if !provider.auth_configurable {
        return Err(AppError::BadRequest(format!(
            "provider {} does not support auth configuration",
            provider_id
        )));
    }

    let current = YtdlpProviderAuthRepo::get_one(db, &provider_id).await?;
    let current_data: Option<OnlineMediaAuthData> = current
        .as_ref()
        .and_then(|s| serde_json::from_value(s.value.clone()).ok());

    let display_name = req
        .display_name
        .or_else(|| current_data.as_ref().map(|d| d.display_name.clone()))
        .unwrap_or_else(|| provider.display_name.clone());

    let cookie = match req.cookie {
        Some(c) => {
            let trimmed = c.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => current_data.as_ref().and_then(|d| d.cookie.clone()),
    };

    let is_enabled = req
        .is_enabled
        .or_else(|| current_data.as_ref().map(|d| d.is_enabled))
        .unwrap_or(true);

    let auth_data = OnlineMediaAuthData {
        display_name: display_name.clone(),
        cookie: cookie.clone(),
        is_enabled,
    };

    let value = serde_json::to_value(&auth_data)
        .map_err(|e| AppError::Internal(format!("failed to serialize auth data: {}", e)))?;

    let updated = YtdlpProviderAuthRepo::upsert(db, &provider_id, value).await?;

    Ok(ok(AuthSettingResponse {
        provider_id,
        display_name,
        requires_auth: provider.requires_auth,
        cookie,
        is_enabled,
        updated_at: Some(updated.updated_at.to_rfc3339()),
    }))
}

// ──────────────────────────────────────────────────────────────────────────────
// Start/retry download handlers
// ──────────────────────────────────────────────────────────────────────────────

async fn resolve_app(db: &DatabaseConnection, target_app_id: &str) -> Result<TargetLib, AppError> {
    let id = Uuid::parse_str(target_app_id).map_err(|_| AppError::BadRequest("无效的视频库 ID".into()))?;
    let video = VideoRepo::get_by_id(db, id).await?.not_found("目标视频库不存在")?;
    Ok(TargetLib {
        id: video.id,
        r#type: video.r#type,
    })
}

async fn resolve_app_by_content_type(db: &DatabaseConnection, content_type: &str) -> Result<TargetLib, AppError> {
    let videos = VideoRepo::list_all(db).await?;
    let video = if matches!(
        content_type,
        "music" | "book" | "audiobook" | "podcast" | "ebook" | "manga"
    ) {
        None
    } else {
        videos.iter().find(|video| video.r#type == content_type)
    }
    .or_else(|| videos.first())
    .not_found("未找到可用的视频库")?;

    Ok(TargetLib {
        id: video.id,
        r#type: video.r#type.clone(),
    })
}

async fn get_provider_auth_cookie(
    db: &DatabaseConnection,
    provider_id: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(provider_id) = provider_id else {
        return Ok(None);
    };
    let Some(setting) = YtdlpProviderAuthRepo::get_one(db, provider_id).await? else {
        return Ok(None);
    };
    let auth_data = serde_json::from_value::<OnlineMediaAuthData>(setting.value).ok();
    Ok(auth_data.and_then(|data| data.is_enabled.then_some(data.cookie).flatten()))
}

#[allow(clippy::too_many_arguments)]
async fn create_online_media_job(
    state: &AppState,
    record_id: Uuid,
    url: &str,
    analysis: &JsonValue,
    download_format: &str,
    media_title: Option<&str>,
    media_year: Option<&str>,
    target_app: &TargetLib,
    user_id: Option<Uuid>,
) -> Result<Uuid, AppError> {
    let provider_id = analysis
        .get("provider")
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_str());
    let auth_cookie = get_provider_auth_cookie(&state.db, provider_id).await?;

    let payload = json!({
        "recordId": record_id.to_string(),
        "url": url,
        "targetAppId": target_app.id.to_string(),
        "analysis": analysis,
        "auth": {
            "cookieHeader": auth_cookie,
        },
        "downloadFormat": download_format,
        "mediaTitle": media_title,
        "mediaYear": media_year,
    });

    let job = JobRepo::create_job_via_bus(
        state,
        "online_media_ingest",
        payload,
        Some(json!({ "recordId": record_id.to_string() })),
        user_id,
    )
    .await?;
    state.bus_notify_job(&job.clone().into());

    Ok(job.id)
}

pub async fn start_online_media_download(
    State(state): State<Arc<AppState>>,
    AuthUser(auth): AuthUser,
    Json(input): Json<StartOnlineMediaDownloadInput>,
) -> Result<Json<ApiResponse<StartDownloadOutput>>, AppError> {
    let user_id = Uuid::parse_str(&auth.user_id).map_err(|_| AppError::Unauthorized("无效的用户 ID".into()))?;

    let analysis = &input.analysis;
    let normalized_url = analysis
        .get("normalizedUrl")
        .and_then(|v| v.as_str())
        .unwrap_or(&input.url);

    let duplicate = DownloadRecordRepo::find_online_media_duplicate(&state.db, normalized_url).await?;
    let duplicate_to_delete = if let Some(dup) = &duplicate {
        if !input.confirm_duplicate {
            let title = metadata_string(dup.app_metadata.as_ref(), "mediaTitle").unwrap_or_else(|| dup.title.clone());
            let message = format!("任务「{title}」已存在，是否重新下载？");
            return Ok(ok(StartDownloadOutput::Duplicate(StartDownloadDuplicateOutput {
                action: "duplicate".into(),
                existing_record_id: dup.id.to_string(),
                existing_status: dup.status.clone(),
                existing_title: Some(title),
                existing_source_site: dup.source_site.clone(),
                existing_source_url: dup.source_url.clone(),
                message,
            })));
        }
        match &input.existing_record_id {
            Some(id) if id == &dup.id.to_string() => Some(dup.id),
            _ => return Err(AppError::Conflict("重复任务状态已变化，请重新确认后再试".into())),
        }
    } else {
        None
    };

    let target_app = resolve_app(&state.db, &input.target_app_id).await?;
    let provider = analysis.get("provider");
    let source_site = analysis
        .get("sourceSite")
        .and_then(|v| v.as_str())
        .or_else(|| provider.and_then(|p| p.get("displayName")).and_then(|v| v.as_str()))
        .or_else(|| provider.and_then(|p| p.get("name")).and_then(|v| v.as_str()));
    let title = input
        .media_title
        .as_deref()
        .or_else(|| analysis.get("title").and_then(|v| v.as_str()))
        .unwrap_or(&input.url);
    let media_title = input
        .media_title
        .clone()
        .or_else(|| analysis.get("title").and_then(|v| v.as_str()).map(String::from));
    let content_type = analysis
        .get("contentType")
        .and_then(|v| v.as_str())
        .unwrap_or(&target_app.r#type);
    let external_id = analysis
        .get("sourceId")
        .and_then(|v| v.as_str())
        .or_else(|| analysis.get("externalId").and_then(|v| v.as_str()))
        .map(String::from);
    let app_metadata = json!({
        "contentType": content_type,
        "mediaTitle": media_title,
        "mediaYear": input.media_year,
        "analysis": analysis,
        "autoOrganize": input.auto_organize,
        "targetAppId": target_app.id.to_string(),
        "targetAppType": target_app.r#type,
        "createdBy": user_id.to_string(),
        "downloadFormat": input.download_format,
        "durationSeconds": analysis.get("durationSeconds").and_then(JsonValue::as_i64),
        "uploader": analysis.get("uploader").and_then(|v| v.as_str()),
        "externalId": external_id,
    });

    let record_id = Uuid::new_v4();
    let thumbnail_url = analysis.get("thumbnailUrl").and_then(|v| v.as_str()).map(String::from);
    let txn = state.db.begin().await?;
    if let Some(existing_id) = duplicate_to_delete {
        DownloadRecordRepo::delete(&txn, existing_id).await?;
    }
    DownloadRecordRepo::create(
        &txn,
        CreateDownloadRecordInput {
            id: record_id,
            title: title.to_string(),
            app_id: "video".into(),
            downloader_type: "yt-dlp".into(),
            source_site: source_site.map(String::from),
            source_url: Some(normalized_url.to_string()),
            app_metadata: Some(app_metadata.clone()),
            thumbnail_url: thumbnail_url.clone(),
            status: "downloading".into(),
            progress: 0.0,
            download_path: Some(String::new()),
        },
    )
    .await?;
    txn.commit().await?;

    // Mirror record into public.download_records so the host Downloads page can see it.
    if let Some(bus) = state.bus_client.get() {
        let req = CreateRecordRequest {
            record_id,
            title: title.to_string(),
            app_id: "video".into(),
            downloader_type: "yt-dlp".into(),
            source_site: source_site.map(String::from),
            source_url: Some(normalized_url.to_string()),
            app_metadata: Some(app_metadata),
            thumbnail_url,
            created_by: Some(user_id.to_string()),
        };
        if let Err(error) = crate::bus_clients::downloader::create_record(bus, &req).await {
            tracing::error!(%error, %record_id, "failed to mirror download record to host — record will not appear in Downloads page");
        }
    }

    let job_id = match create_online_media_job(
        &state,
        record_id,
        &input.url,
        analysis,
        &input.download_format,
        input.media_title.as_deref(),
        input.media_year.as_deref(),
        &target_app,
        Some(user_id),
    )
    .await
    {
        Ok(job_id) => job_id,
        Err(err) => {
            let message = format!("创建下载任务失败: {err}");
            push_downloader_status(
                &state,
                UpdateDownloaderStatusRequest {
                    record_id,
                    status: Some("failed".into()),
                    progress: None,
                    downloaded_bytes: None,
                    download_speed: Some(0),
                    eta_seconds: None,
                    thumbnail_url: None,
                    error_message: Some(message),
                },
            )
            .await;
            return Err(err);
        }
    };

    info!(record_id = %record_id, job_id = %job_id, "Online media download started");

    Ok(ok(StartDownloadOutput::Success(StartDownloadSuccessOutput {
        action: "started".into(),
        record_id: record_id.to_string(),
        job_id: job_id.to_string(),
    })))
}

pub async fn retry_online_media_download(
    State(state): State<Arc<AppState>>,
    AuthUser(auth): AuthUser,
    Json(input): Json<RetryDownloadInput>,
) -> Result<Json<ApiResponse<StartDownloadOutput>>, AppError> {
    let record_id = Uuid::parse_str(&input.record_id).map_err(|_| AppError::BadRequest("无效的记录 ID".into()))?;
    let record = DownloadRecordRepo::get_model_by_id(&state.db, record_id)
        .await?
        .not_found("下载记录不存在")?;

    if record.app_id != "video" || record.downloader_type != "yt-dlp" {
        return Err(AppError::BadRequest("只有在线视频下载任务支持重试".into()));
    }
    if record.status != "failed" {
        return Err(AppError::BadRequest("只有失败的下载任务可以重试".into()));
    }
    let metadata = record.app_metadata.as_ref();
    if let Some(created_by) = metadata_string(metadata, "createdBy")
        && created_by != auth.user_id
    {
        return Err(AppError::Forbidden("无权操作此下载记录".into()));
    }

    let analysis = metadata
        .and_then(|value| value.get("analysis"))
        .cloned()
        .bad_request("缺少重试所需的下载参数")?;
    let source_url = record.source_url.clone().bad_request("缺少重试所需的下载参数")?;
    let target_app = if let Some(target_app_id) = metadata_string(metadata, "targetAppId") {
        resolve_app(&state.db, &target_app_id).await?
    } else if let Some(content_type) = metadata_string(metadata, "contentType") {
        resolve_app_by_content_type(&state.db, &content_type).await?
    } else {
        return Err(AppError::BadRequest("缺少重试所需的视频库信息".into()));
    };

    // Reset status via bus instead of direct repo write
    push_downloader_status(
        &state,
        UpdateDownloaderStatusRequest {
            record_id,
            status: Some("downloading".into()),
            progress: Some(0.0),
            downloaded_bytes: None,
            download_speed: None,
            eta_seconds: None,
            thumbnail_url: None,
            error_message: None,
        },
    )
    .await;

    let retry_media_title = metadata_string(metadata, "mediaTitle");
    let retry_media_year = metadata_string(metadata, "mediaYear");
    let user_id = Uuid::parse_str(&auth.user_id).map_err(|_| AppError::Unauthorized("无效的用户 ID".into()))?;
    let job_id = match create_online_media_job(
        &state,
        record_id,
        &source_url,
        &analysis,
        "auto",
        retry_media_title.as_deref(),
        retry_media_year.as_deref(),
        &target_app,
        Some(user_id),
    )
    .await
    {
        Ok(job_id) => job_id,
        Err(err) => {
            let message = format!("创建下载任务失败: {err}");
            push_downloader_status(
                &state,
                UpdateDownloaderStatusRequest {
                    record_id,
                    status: Some("failed".into()),
                    progress: None,
                    downloaded_bytes: None,
                    download_speed: Some(0),
                    eta_seconds: None,
                    thumbnail_url: None,
                    error_message: Some(message),
                },
            )
            .await;
            return Err(err);
        }
    };

    info!(record_id = %record_id, job_id = %job_id, "Online media download retried");

    Ok(ok(StartDownloadOutput::Success(StartDownloadSuccessOutput {
        action: "restarted".into(),
        record_id: record_id.to_string(),
        job_id: job_id.to_string(),
    })))
}
