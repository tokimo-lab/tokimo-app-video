use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;
use uuid::Uuid;

use crate::db::ApiDateTimeExt;
use crate::AppState;
use crate::apps::subscriptions::repos::subscription_repo::{
    CreateSubscriptionInput, SubscriptionRepo, UpdateSubscriptionInput,
};
use crate::error::AppError;
use crate::handlers::{ok, user::AuthUser};
use crate::services::storage::{StorageProvider, UploadOptions};

// ── Log types ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionLogEntry {
    pub timestamp: String,
    pub subscription_id: String,
    pub run_id: String,
    pub phase: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SubscriptionRunSummary {
    run_id: String,
    started_at: String,
    completed_at: Option<String>,
    total_found: i64,
    after_filter: i64,
    matched: bool,
    matched_torrent: Option<String>,
    downloaded: bool,
    is_running: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionDebugInfo {
    subscription: crate::apps::subscriptions::repos::subscription_repo::SubscriptionDto,
    total_runs: usize,
    successful_downloads: usize,
    last_matched_at: Option<String>,
    active_run_id: Option<String>,
    recent_runs: Vec<SubscriptionRunSummary>,
}

// ── Log helpers ──────────────────────────────────────────────────────────────

fn subscription_log_key(subscription_id: &str) -> String {
    format!("logs/subscription/{subscription_id}.jsonl")
}

async fn append_log(
    storage: &Arc<dyn StorageProvider>,
    sub_id: &str,
    run_id: &str,
    phase: &str,
    message: &str,
    details: Option<serde_json::Value>,
) {
    let key = subscription_log_key(sub_id);
    let entry = SubscriptionLogEntry {
        timestamp: chrono::Utc::now().to_api_datetime(),
        subscription_id: sub_id.to_string(),
        run_id: run_id.to_string(),
        phase: phase.to_string(),
        message: message.to_string(),
        details,
    };
    let new_line = format!("{}\n", serde_json::to_string(&entry).unwrap_or_default());

    let existing = storage.download(&key).await.unwrap_or_default();
    let mut content = existing.to_vec();
    content.extend_from_slice(new_line.as_bytes());

    if let Err(e) = storage
        .upload(
            &key,
            Bytes::from(content),
            Some(UploadOptions {
                content_type: Some("application/x-ndjson".into()),
            }),
        )
        .await
    {
        error!(subscription_id = sub_id, "Failed to write subscription log: {e}");
    }
}

async fn read_all_logs(storage: &Arc<dyn StorageProvider>, subscription_id: &str) -> Vec<SubscriptionLogEntry> {
    let key = subscription_log_key(subscription_id);
    let content = match storage.download(&key).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => return vec![],
    };
    content
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

async fn read_recent_logs(
    storage: &Arc<dyn StorageProvider>,
    subscription_id: &str,
    limit: usize,
) -> Vec<SubscriptionLogEntry> {
    let entries = read_all_logs(storage, subscription_id).await;
    let start = entries.len().saturating_sub(limit);
    entries[start..].to_vec()
}

async fn get_run_summaries(
    storage: &Arc<dyn StorageProvider>,
    subscription_id: &str,
    limit: usize,
) -> Vec<SubscriptionRunSummary> {
    let logs = read_all_logs(storage, subscription_id).await;
    let mut groups: std::collections::HashMap<String, Vec<SubscriptionLogEntry>> = std::collections::HashMap::new();
    let mut run_order: Vec<String> = Vec::new();

    for entry in logs {
        if !groups.contains_key(&entry.run_id) {
            run_order.push(entry.run_id.clone());
        }
        groups.entry(entry.run_id.clone()).or_default().push(entry);
    }

    let mut summaries = Vec::new();
    for run_id in &run_order {
        let Some(entries) = groups.get(run_id) else {
            continue;
        };

        let start = entries.iter().find(|e| e.phase == "start");
        let completed = entries.iter().find(|e| e.phase == "completed");
        let err = entries.iter().find(|e| e.phase == "error");
        let downloading = entries.iter().find(|e| e.phase == "downloading");
        let total_found = entries
            .iter()
            .find(|e| e.phase == "search_result")
            .and_then(|e| e.details.as_ref())
            .and_then(|d| d.get("count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let after_filter = entries
            .iter()
            .find(|e| e.phase == "filter_result")
            .and_then(|e| e.details.as_ref())
            .and_then(|d| d.get("count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        summaries.push(SubscriptionRunSummary {
            run_id: run_id.clone(),
            started_at: start.map(|e| e.timestamp.clone()).unwrap_or_default(),
            completed_at: completed.map(|e| e.timestamp.clone()),
            total_found,
            after_filter,
            matched: downloading.is_some(),
            matched_torrent: downloading
                .and_then(|e| e.details.as_ref())
                .and_then(|d| d.get("torrentName"))
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string),
            downloaded: completed.is_some() && downloading.is_some(),
            is_running: false,
            error: err.map(|e| e.message.clone()),
        });
    }

    let start = summaries.len().saturating_sub(limit);
    let mut result = summaries[start..].to_vec();
    result.reverse();
    result
}

async fn delete_subscription_logs(storage: &Arc<dyn StorageProvider>, subscription_id: &str) {
    let key = subscription_log_key(subscription_id);
    let _ = storage.delete(&key).await;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn check_ownership(owner: Option<&str>, user_id: &str) -> Result<(), Response> {
    if owner.is_some_and(|o| o != user_id) {
        Err(AppError::Forbidden("无权访问此资源".into()).into_response())
    } else {
        Ok(())
    }
}

// ── Endpoints ────────────────────────────────────────────────────────────────

pub async fn list(State(state): State<Arc<AppState>>, auth: AuthUser) -> Response {
    match SubscriptionRepo::list(&state.db, &auth.user_id).await {
        Ok(subs) => ok(subs).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn get_by_id(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Response {
    match SubscriptionRepo::get_by_id(&state.db, &id).await {
        Ok(Some(sub)) => {
            if let Err(e) = check_ownership(sub.created_by.as_deref(), &auth.user_id) {
                return e;
            }
            ok(sub).into_response()
        }
        Ok(None) => AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateSubscriptionInput>,
) -> Response {
    match SubscriptionRepo::create(&state.db, body, &auth.user_id).await {
        Ok(sub) => {
            let sub_id = sub.id.clone();
            let run_id = Uuid::new_v4().to_string();
            if let Ok(mut runs) = state.active_subscription_runs.write() {
                runs.insert(sub_id.clone(), run_id.clone());
            }
            let active_runs = Arc::clone(&state.active_subscription_runs);
            let db = state.db.clone();
            let storage = Arc::clone(&state.storage);
            let interval = sub.interval_minutes;
            tokio::spawn(async move {
                append_log(&storage, &sub_id, &run_id, "start", "订阅执行开始", None).await;
                let _ = SubscriptionRepo::update_timestamps(&db, &sub_id, interval).await;
                append_log(&storage, &sub_id, &run_id, "completed", "执行完成", None).await;
                if let Ok(mut runs) = active_runs.write()
                    && runs.get(&sub_id).is_some_and(|r| r == &run_id)
                {
                    runs.remove(&sub_id);
                }
            });
            ok(sub).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(mut body): Json<UpdateSubscriptionInput>,
) -> Response {
    match SubscriptionRepo::get_raw(&state.db, &id).await {
        Ok(Some(raw)) => {
            if let Err(e) =
                check_ownership(raw.created_by.map(|u| u.to_string()).as_deref(), &auth.user_id)
            {
                return e;
            }
        }
        Ok(None) => return AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    }

    body.id = id.clone();
    match SubscriptionRepo::update(&state.db, &id, body).await {
        Ok(Some(sub)) => ok(sub).into_response(),
        Ok(None) => AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn delete(State(state): State<Arc<AppState>>, auth: AuthUser, Path(id): Path<String>) -> Response {
    match SubscriptionRepo::get_raw(&state.db, &id).await {
        Ok(Some(raw)) => {
            if let Err(e) =
                check_ownership(raw.created_by.map(|u| u.to_string()).as_deref(), &auth.user_id)
            {
                return e;
            }
        }
        Ok(None) => return AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    }

    match SubscriptionRepo::delete(&state.db, &id).await {
        Ok(true) => {
            delete_subscription_logs(&state.storage, &id).await;
            ok(SuccessBody { success: true }).into_response()
        }
        Ok(false) => AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => e.into_response(),
    }
}

#[derive(Serialize)]
struct SuccessBody {
    success: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteResponse {
    success: bool,
    run_id: String,
}

pub async fn execute(State(state): State<Arc<AppState>>, auth: AuthUser, Path(id): Path<String>) -> Response {
    let sub = match SubscriptionRepo::get_raw(&state.db, &id).await {
        Ok(Some(raw)) => {
            if let Err(e) =
                check_ownership(raw.created_by.map(|u| u.to_string()).as_deref(), &auth.user_id)
            {
                return e;
            }
            raw
        }
        Ok(None) => return AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    };

    let run_id = Uuid::new_v4().to_string();
    let sub_id = id.clone();

    if let Ok(mut runs) = state.active_subscription_runs.write() {
        runs.insert(sub_id.clone(), run_id.clone());
    }

    let active_runs = Arc::clone(&state.active_subscription_runs);
    let db = state.db.clone();
    let storage = Arc::clone(&state.storage);
    let run_id_clone = run_id.clone();
    let interval: i32 = sub.interval_minutes.parse().unwrap_or(30);

    tokio::spawn(async move {
        append_log(&storage, &sub_id, &run_id_clone, "start", "订阅执行开始", None).await;
        if let Err(e) = SubscriptionRepo::update_timestamps(&db, &sub_id, interval).await {
            error!("update timestamps failed: {e}");
        }
        append_log(&storage, &sub_id, &run_id_clone, "completed", "执行完成", None).await;
        if let Ok(mut runs) = active_runs.write()
            && runs.get(&sub_id).is_some_and(|r| r == &run_id_clone)
        {
            runs.remove(&sub_id);
        }
    });

    ok(ExecuteResponse { success: true, run_id }).into_response()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActiveRunResponse {
    run_id: Option<String>,
}

pub async fn get_active_run_id(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Response {
    match SubscriptionRepo::get_raw(&state.db, &id).await {
        Ok(Some(raw)) => {
            if let Err(e) =
                check_ownership(raw.created_by.map(|u| u.to_string()).as_deref(), &auth.user_id)
            {
                return e;
            }
        }
        Ok(None) => return AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    }

    let run_id = state
        .active_subscription_runs
        .read()
        .ok()
        .and_then(|runs| runs.get(&id).cloned());

    ok(ActiveRunResponse { run_id }).into_response()
}

pub async fn get_debug_info(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Response {
    match SubscriptionRepo::get_raw(&state.db, &id).await {
        Ok(Some(raw)) => {
            if let Err(e) =
                check_ownership(raw.created_by.map(|u| u.to_string()).as_deref(), &auth.user_id)
            {
                return e;
            }
        }
        Ok(None) => return AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    }

    let sub = match SubscriptionRepo::get_by_id(&state.db, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    };

    let active_run_id = state
        .active_subscription_runs
        .read()
        .ok()
        .and_then(|runs| runs.get(&id).cloned());

    let mut recent_runs = get_run_summaries(&state.storage, &id, 20).await;
    if let Some(ref active_id) = active_run_id {
        for run in &mut recent_runs {
            if run.run_id == *active_id {
                run.is_running = true;
            }
        }
    }

    let total_runs = recent_runs.len();
    let successful_downloads = recent_runs.iter().filter(|r| r.downloaded).count();
    let last_matched = recent_runs.iter().find(|r| r.downloaded);

    ok(SubscriptionDebugInfo {
        subscription: sub,
        total_runs,
        successful_downloads,
        last_matched_at: last_matched.map(|r| r.started_at.clone()),
        active_run_id,
        recent_runs,
    })
    .into_response()
}

#[derive(Deserialize)]
pub struct LogsQuery {
    pub limit: Option<usize>,
}

pub async fn get_recent_logs(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<LogsQuery>,
) -> Response {
    match SubscriptionRepo::get_raw(&state.db, &id).await {
        Ok(Some(raw)) => {
            if let Err(e) =
                check_ownership(raw.created_by.map(|u| u.to_string()).as_deref(), &auth.user_id)
            {
                return e;
            }
        }
        Ok(None) => return AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    }

    let limit = query.limit.unwrap_or(200);
    let logs = read_recent_logs(&state.storage, &id, limit).await;
    ok(logs).into_response()
}

pub async fn get_run_logs(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((id, run_id)): Path<(String, String)>,
) -> Response {
    match SubscriptionRepo::get_raw(&state.db, &id).await {
        Ok(Some(raw)) => {
            if let Err(e) =
                check_ownership(raw.created_by.map(|u| u.to_string()).as_deref(), &auth.user_id)
            {
                return e;
            }
        }
        Ok(None) => return AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    }

    let all = read_recent_logs(&state.storage, &id, 10000).await;
    let run_logs: Vec<SubscriptionLogEntry> = all.into_iter().filter(|e| e.run_id == run_id).collect();
    ok(run_logs).into_response()
}

pub async fn get_episode_progress(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Response {
    match SubscriptionRepo::get_raw(&state.db, &id).await {
        Ok(Some(raw)) => {
            if let Err(e) =
                check_ownership(raw.created_by.map(|u| u.to_string()).as_deref(), &auth.user_id)
            {
                return e;
            }
        }
        Ok(None) => return AppError::NotFound("订阅不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    }

    match SubscriptionRepo::get_episode_progress(&state.db, &id).await {
        Ok(progress) => ok(progress).into_response(),
        Err(e) => e.into_response(),
    }
}
