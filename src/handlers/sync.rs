use axum::{
    extract::{Path, State},
    response::Json,
};
use futures::FutureExt;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::db::ApiDateTimeExt;
use crate::db::models::video::VideoSyncStatusOutput;
use crate::db::repos::media::VideoRepo;
use crate::error::AppError;
use crate::error::OptionExt;
use crate::handlers::user::AuthUser;
use crate::handlers::{ApiResponse, ok};
use crate::services::media::app_sync::AppSyncService;

use super::{VideoSyncInput, parse_uuid};

/// POST /api/apps/video/{id}/sync
pub async fn sync_video(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    auth: AuthUser,
    body: Option<Json<VideoSyncInput>>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let caller_user_id: Uuid = auth
        .user_id
        .parse()
        .map_err(|_| AppError::Unauthorized("invalid user_id in auth token".into()))?;

    let uid: Uuid = id
        .parse()
        .map_err(|_| AppError::BadRequest("invalid video id".into()))?;

    let video = VideoRepo::get_by_id(&state.db, uid)
        .await?
        .not_found(format!("video {id} not found"))?;

    let clear_data = body.and_then(|b| b.clear_data).unwrap_or(false);

    if video.sync_status == "syncing" && !clear_data {
        warn!(
            "Video {} is already syncing (may be stuck from a previous crash), allowing re-sync",
            video.name
        );
    }

    // Clear data synchronously so frontend sees empty state immediately
    if clear_data {
        let client = state.bus_client.get().expect("bus_client not initialized");
        AppSyncService::clear_library_data(&state.db, client, uid, &video.r#type, caller_user_id).await?;
    }

    VideoRepo::update_sync_status(&state.db, uid, "syncing", None).await?;

    let db = state.db.clone();
    let sources = state.sources.clone();
    let storage = state.storage().clone();
    let http_client = state.http_client.clone();

    tokio::spawn(async move {
        let db2 = db.clone();
        let result = std::panic::AssertUnwindSafe(AppSyncService::execute_video_sync(
            &db,
            &sources,
            &storage,
            state.bus_client.clone(),
            uid,
            false,
            http_client,
            caller_user_id,
        ))
        .catch_unwind()
        .await;

        match result {
            Ok(Ok(r)) => {
                info!("video sync completed, {} jobs dispatched", r.total_jobs);
            }
            Ok(Err(e)) => {
                error!("video sync failed: {e}");
            }
            Err(_panic) => {
                error!("video sync panicked — check logs for backtrace");
                let _ = VideoRepo::update_sync_status(&db2, uid, "failed", None).await;
            }
        }
    });

    Ok(ok(serde_json::json!({ "success": true })))
}

/// GET /api/apps/video/{id}/sync-status
pub async fn get_video_sync_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<VideoSyncStatusOutput>>, AppError> {
    let uid = parse_uuid(&id)?;
    let (status, last_sync_at) = VideoRepo::get_sync_status(&state.db, uid)
        .await?
        .not_found(format!("video {id} not found"))?;
    Ok(ok(VideoSyncStatusOutput {
        video_id: uid.to_string(),
        status,
        last_sync_at: last_sync_at.to_api_datetime(),
    }))
}

/// GET /api/apps/video/sync-statuses
pub async fn get_all_video_sync_statuses(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<VideoSyncStatusOutput>>>, AppError> {
    let rows = VideoRepo::list_all(&state.db).await?;
    let statuses: Vec<VideoSyncStatusOutput> = rows
        .into_iter()
        .map(|m| VideoSyncStatusOutput {
            video_id: m.id.to_string(),
            status: m.sync_status,
            last_sync_at: m.last_sync_at.to_api_datetime(),
        })
        .collect();
    Ok(ok(statuses))
}

/// GET /api/apps/video/scraping-settings
pub async fn get_video_scraping_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    use crate::config::ScrapingSettings;
    use crate::db::repos::scrape_settings_repo::ScrapeSettingsRepo;
    let settings = ScrapeSettingsRepo::get::<ScrapingSettings>(&state.db).await?;
    Ok(ok(serde_json::to_value(settings)?))
}

/// PUT /api/apps/video/scraping-settings
pub async fn update_video_scraping_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    use crate::config::ScrapingSettings;
    use crate::db::repos::scrape_settings_repo::ScrapeSettingsRepo;
    let settings: ScrapingSettings = serde_json::from_value(body)?;
    ScrapeSettingsRepo::set::<ScrapingSettings>(&state.db, &settings).await?;
    Ok(ok(serde_json::to_value(settings)?))
}
