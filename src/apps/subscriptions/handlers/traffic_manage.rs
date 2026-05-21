use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokimo_package_utils::path::normalize_local_path;
use tracing::info;
use uuid::Uuid;

use crate::AppState;
use crate::apps::subscriptions::repos::traffic_manage_repo::{
    TrafficManageRepo, TrafficManageSettingsDto, UpdateTrafficSettingsInput,
};
use crate::db::entities::pt_sites;
use crate::error::AppError;
use crate::handlers::{ok, user::AuthUser};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsQuery {
    pub pt_site_id: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

pub async fn get_settings(State(state): State<Arc<AppState>>, _auth: AuthUser) -> impl IntoResponse {
    match TrafficManageRepo::get_settings(&state.db).await {
        Ok(s) => ok(TrafficManageSettingsDto::from(s)).into_response(),
        Err(e) => e.into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsBody {
    pub download_path: Option<String>,
    pub min_free_disk_space_gb: Option<i32>,
    pub stats_window_minutes: Option<i32>,
    pub max_upload_rate_mbps: Option<i32>,
    pub max_active_torrents: Option<i32>,
    pub scan_interval_minutes: Option<i32>,
    pub cleanup_interval_minutes: Option<i32>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub download_client_id: Option<Option<String>>,
    pub is_enabled: Option<bool>,
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(mut body): Json<UpdateSettingsBody>,
) -> impl IntoResponse {
    if let Some(ref p) = body.download_path {
        match normalize_local_path(p) {
            Ok(canonical) => body.download_path = Some(canonical),
            Err(e) => {
                return AppError::BadRequest(format!("downloadPath: {e}")).into_response();
            }
        }
    }

    let input = UpdateTrafficSettingsInput {
        download_path: body.download_path,
        min_free_disk_space_gb: body.min_free_disk_space_gb,
        stats_window_minutes: body.stats_window_minutes,
        max_upload_rate_mbps: body.max_upload_rate_mbps,
        max_active_torrents: body.max_active_torrents,
        scan_interval_minutes: body.scan_interval_minutes,
        cleanup_interval_minutes: body.cleanup_interval_minutes,
        download_client_id: body.download_client_id,
        is_enabled: body.is_enabled,
    };

    match TrafficManageRepo::upsert_settings(&state.db, input).await {
        Ok(s) => ok(TrafficManageSettingsDto::from(s)).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn get_logs(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Query(query): Query<LogsQuery>,
) -> impl IntoResponse {
    let pt_site_id = query.pt_site_id.and_then(|s| Uuid::parse_str(&s).ok());
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    match TrafficManageRepo::get_logs(&state.db, pt_site_id, limit, offset).await {
        Ok((items, total)) => ok(LogsResponse { items, total }).into_response(),
        Err(e) => e.into_response(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogsResponse {
    items: Vec<crate::apps::subscriptions::repos::traffic_manage_repo::TrafficManageLogDto>,
    total: u64,
}

pub async fn get_stats(State(state): State<Arc<AppState>>, _auth: AuthUser) -> impl IntoResponse {
    match TrafficManageRepo::get_stats(&state.db).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn trigger_scan(State(state): State<Arc<AppState>>, _auth: AuthUser) -> impl IntoResponse {
    let settings = match TrafficManageRepo::get_settings(&state.db).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    if !settings.is_enabled {
        return AppError::BadRequest("流量管理未启用".into()).into_response();
    }

    let sites = match pt_sites::Entity::find()
        .filter(pt_sites::Column::TrafficManageEnabled.eq(true))
        .count(&state.db)
        .await
    {
        Ok(c) => c,
        Err(e) => return AppError::from(e).into_response(),
    };

    info!("[traffic-manage] manual scan triggered: {} eligible sites", sites);
    ok(ScanResult { downloaded: 0 }).into_response()
}

#[derive(Serialize)]
struct ScanResult {
    downloaded: u32,
}

pub async fn trigger_cleanup(State(state): State<Arc<AppState>>, _auth: AuthUser) -> impl IntoResponse {
    let settings = match TrafficManageRepo::get_settings(&state.db).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    if !settings.is_enabled {
        return AppError::BadRequest("流量管理未启用".into()).into_response();
    }

    // V4 schema: no uploaded_size/ratio fields, simplified cleanup
    info!("[traffic-manage] manual cleanup triggered (V4 schema)");
    ok(CleanupResult { cleaned: 0 }).into_response()
}

#[derive(Serialize)]
struct CleanupResult {
    cleaned: u32,
}
