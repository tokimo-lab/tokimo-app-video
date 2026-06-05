#![allow(dead_code)]

use std::collections::HashMap;

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokimo_bus_client::BusClient;
use tokimo_bus_protocol::CallerCtx;
use uuid::Uuid;

use crate::db::entities::jobs;
use crate::error::AppError;

pub type Job = jobs::Model;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJobRequest {
    #[serde(rename = "kind")]
    pub job_type: String,
    pub params: JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_job_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
}

impl CreateJobRequest {
    pub fn new(job_type: impl Into<String>, params: JsonValue) -> Self {
        Self {
            job_type: job_type.into(),
            params,
            data: None,
            parent_job_id: None,
            task_type: None,
            dedupe_key: None,
            priority: None,
        }
    }

    pub fn with_data(mut self, data: Option<JsonValue>) -> Self {
        self.data = data;
        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryJobsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub job_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryJobsResponse {
    pub items: Vec<JobView>,
    pub total: i64,
    pub page: u64,
    pub page_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelJobRequest {
    pub id: Uuid,
    #[serde(default = "default_cascade_children")]
    pub cascade_children: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl CancelJobRequest {
    pub fn new(id: Uuid) -> Self {
        Self {
            id,
            cascade_children: true,
            reason: None,
        }
    }
}

fn default_cascade_children() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelJobResponse {
    pub cancelled: bool,
    pub cancelled_children: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchChildrenRequest {
    pub parent_job_id: Uuid,
    pub items: Vec<CreateJobRequest>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum BatchChildrenResponse {
    Jobs(Vec<JobView>),
    Wrapped { jobs: Vec<JobView> },
    Inserted { inserted: u64 },
    Other(JsonValue),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusRequest {
    pub job_id: Uuid,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgressRequest {
    job_id: Uuid,
    progress: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<JsonValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub job_type: String,
    pub status: String,
    pub user_id: Option<Uuid>,
    pub parent_job_id: Option<Uuid>,
    pub task_type: Option<String>,
    pub params: JsonValue,
    pub data: Option<JsonValue>,
    pub progress: i32,
    pub priority: i32,
    pub error: Option<String>,
    pub started_at: Option<DateTime<FixedOffset>>,
    pub completed_at: Option<DateTime<FixedOffset>>,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
}

impl From<JobView> for Job {
    fn from(view: JobView) -> Self {
        Self {
            id: view.id,
            r#type: view.job_type,
            status: view.status,
            user_id: view.user_id,
            parent_job_id: view.parent_job_id,
            task_type: view.task_type,
            params: view.params,
            data: view.data,
            progress: view.progress,
            retry_count: 0,
            max_retries: 3,
            error: view.error,
            started_at: view.started_at,
            completed_at: view.completed_at,
            created_at: view.created_at,
            updated_at: view.updated_at,
            dedupe_key: None,
            alias_job_id: None,
            priority: view.priority,
        }
    }
}

pub fn video_caller(user_id: Option<Uuid>) -> CallerCtx {
    CallerCtx {
        user_id: user_id.map(|id| id.to_string()),
        request_id: Uuid::new_v4().to_string(),
        workspace: None,
        caller_app_id: Some("video".to_string()),
    }
}

/// CallerCtx for service-level operations (no user context).
pub fn service_caller() -> CallerCtx {
    video_caller(None)
}

/// Build a `JobFilter` that matches jobs belonging to a specific video library.
pub fn video_library_filter(library_id: Uuid, status: Option<&str>) -> JobFilter {
    let mut params_match = HashMap::new();
    params_match.insert("videoId".to_string(), library_id.to_string());
    JobFilter {
        status: status.map(String::from),
        params_match: Some(params_match),
        ..Default::default()
    }
}

pub async fn create(client: &BusClient, caller: CallerCtx, request: CreateJobRequest) -> Result<Job, AppError> {
    let response = invoke_json(client, "create", caller, &request).await?;
    serde_json::from_slice::<JobView>(&response)
        .map(Job::from)
        .map_err(|error| AppError::Internal(format!("jobs.create decode: {error}")))
}

pub async fn query(
    client: &BusClient,
    caller: CallerCtx,
    request: QueryJobsRequest,
) -> Result<QueryJobsResponse, AppError> {
    let response = invoke_json(client, "query", caller, &request).await?;
    serde_json::from_slice(&response).map_err(|error| AppError::Internal(format!("jobs.query decode: {error}")))
}

pub async fn cancel(client: &BusClient, caller: CallerCtx, request: CancelJobRequest) -> Result<(), AppError> {
    let _ = invoke_json(client, "cancel", caller, &request).await?;
    Ok(())
}

pub async fn batch_children(
    client: &BusClient,
    caller: CallerCtx,
    parent_id: Uuid,
    jobs: Vec<CreateJobRequest>,
) -> Result<Vec<Job>, AppError> {
    let response = invoke_json(
        client,
        "batch_children",
        caller,
        &BatchChildrenRequest {
            parent_job_id: parent_id,
            items: jobs,
        },
    )
    .await?;
    let Ok(parsed) = serde_json::from_slice::<BatchChildrenResponse>(&response) else {
        return Ok(Vec::new());
    };
    let jobs = match parsed {
        BatchChildrenResponse::Jobs(items) | BatchChildrenResponse::Wrapped { jobs: items } => {
            items.into_iter().map(Job::from).collect()
        }
        BatchChildrenResponse::Inserted { .. } | BatchChildrenResponse::Other(_) => Vec::new(),
    };
    Ok(jobs)
}

pub async fn update_status(
    client: &BusClient,
    caller: CallerCtx,
    request: UpdateStatusRequest,
) -> Result<Job, AppError> {
    let response = invoke_json(client, "update_status", caller, &request).await?;
    serde_json::from_slice::<JobView>(&response)
        .map(Job::from)
        .map_err(|error| AppError::Internal(format!("jobs.update_status decode: {error}")))
}

pub async fn update_progress(
    client: &BusClient,
    caller: CallerCtx,
    job_id: Uuid,
    progress: i32,
    progress_data: Option<JsonValue>,
) -> Result<Job, AppError> {
    let request = UpdateProgressRequest {
        job_id,
        progress,
        data: progress_data,
    };
    let response = invoke_json(client, "update_progress", caller, &request).await?;
    serde_json::from_slice::<JobView>(&response)
        .map(Job::from)
        .map_err(|error| AppError::Internal(format!("jobs.update_progress decode: {error}")))
}

// ── Filter-based types + methods ───────────────────────────────────────────────

/// OS-layer generic job filter — matches host-side `JobFilter`.
/// Business semantics (e.g. `{"videoId": "..."}`) are encoded in `params_match`;
/// the bus layer only does JSONB equality matching.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobFilter {
    pub status: Option<String>,
    pub job_type: Option<String>,
    pub params_match: Option<HashMap<String, String>>,
    pub parents_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelByFilterRequest {
    filter: JobFilter,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelByFilterResponse {
    pub cancelled: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressSummaryRequest {
    filter: JobFilter,
    job_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressSummaryResponse {
    pub total: i64,
    pub completed: i64,
    pub running: i64,
    pub pending: i64,
    pub failed: i64,
    pub tasks: Vec<TaskProgressRowView>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgressRowView {
    #[serde(rename = "type")]
    pub job_type: String,
    pub completed: i64,
    pub running: i64,
    pub pending: i64,
    pub failed: i64,
    #[serde(rename = "runningMeta", alias = "runningData")]
    pub running_data: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupRequest {
    filter: JobFilter,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResponse {
    pub deleted: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreemptRequest {
    filter: JobFilter,
    reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreemptResponse {
    pub cancelled_ids: Vec<Uuid>,
}

/// Bulk-cancel jobs matching the given filter (app_id scoped via caller).
pub async fn cancel_by_filter(client: &BusClient, caller: CallerCtx, filter: JobFilter) -> Result<u64, AppError> {
    let response = invoke_json(client, "cancel_by_filter", caller, &CancelByFilterRequest { filter }).await?;
    let resp: CancelByFilterResponse = serde_json::from_slice(&response)
        .map_err(|e| AppError::Internal(format!("jobs.cancel_by_filter decode: {e}")))?;
    Ok(resp.cancelled)
}

/// Aggregated progress summary for jobs matching the filter.
pub async fn progress_summary(
    client: &BusClient,
    caller: CallerCtx,
    filter: JobFilter,
    job_types: Vec<String>,
) -> Result<ProgressSummaryResponse, AppError> {
    let response = invoke_json(
        client,
        "progress_summary",
        caller,
        &ProgressSummaryRequest { filter, job_types },
    )
    .await?;
    serde_json::from_slice(&response).map_err(|e| AppError::Internal(format!("jobs.progress_summary decode: {e}")))
}

/// Delete finished (completed/cancelled/failed) jobs matching the filter.
pub async fn cleanup(client: &BusClient, caller: CallerCtx, filter: JobFilter) -> Result<u64, AppError> {
    let response = invoke_json(client, "cleanup", caller, &CleanupRequest { filter }).await?;
    let resp: CleanupResponse =
        serde_json::from_slice(&response).map_err(|e| AppError::Internal(format!("jobs.cleanup decode: {e}")))?;
    Ok(resp.deleted)
}

/// Preempt parent scan jobs matching the filter.
pub async fn preempt(
    client: &BusClient,
    caller: CallerCtx,
    filter: JobFilter,
    reason: &str,
) -> Result<Vec<Uuid>, AppError> {
    let response = invoke_json(
        client,
        "preempt",
        caller,
        &PreemptRequest {
            filter,
            reason: reason.to_string(),
        },
    )
    .await?;
    let resp: PreemptResponse =
        serde_json::from_slice(&response).map_err(|e| AppError::Internal(format!("jobs.preempt decode: {e}")))?;
    Ok(resp.cancelled_ids)
}

async fn invoke_json<T: Serialize>(
    client: &BusClient,
    method: &str,
    caller: CallerCtx,
    request: &T,
) -> Result<Vec<u8>, AppError> {
    let payload =
        serde_json::to_vec(request).map_err(|error| AppError::Internal(format!("jobs.{method} encode: {error}")))?;
    client
        .invoke("jobs", method, payload, caller)
        .await
        .map_err(|error| AppError::Internal(format!("jobs.{method} via bus: {error}")))
}

/// Register a single job handler mapping with the main server.
///
/// The main server infers `appId` from the bus caller context (broker-stamped),
/// so the sidecar cannot spoof its identity.
pub async fn register_handler(client: &BusClient, job_type: &str, method: &str) -> Result<(), AppError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Req<'a> {
        job_type: &'a str,
        method: &'a str,
    }
    let _ = invoke_json(
        client,
        "register_handler",
        CallerCtx::default(),
        &Req { job_type, method },
    )
    .await?;
    Ok(())
}
