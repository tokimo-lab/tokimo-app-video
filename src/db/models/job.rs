use crate::db::ApiDateTimeExt;
use serde::Serialize;

use crate::db::entities::jobs;

/// Job output DTO for API responses.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JobOutput {
    pub id: String,
    pub r#type: String,
    pub status: String,
    pub payload: serde_json::Value,
    pub meta: Option<serde_json::Value>,
    pub progress: i32,
    pub retry_count: i32,
    pub max_retries: i32,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub parent_job_id: Option<String>,
    pub child_count: i64,
}

impl From<jobs::Model> for JobOutput {
    fn from(m: jobs::Model) -> Self {
        let parent_job_id = m.parent_job_id.map(|u| u.to_string());
        Self {
            id: m.id.to_string(),
            r#type: m.r#type,
            status: m.status,
            payload: m.payload,
            meta: m.meta,
            progress: m.progress,
            retry_count: m.retry_count,
            max_retries: m.max_retries,
            error: m.error,
            started_at: m.started_at.to_api_datetime(),
            completed_at: m.completed_at.to_api_datetime(),
            created_at: m.created_at.to_api_datetime(),
            updated_at: m.updated_at.to_api_datetime(),
            parent_job_id,
            child_count: 0,
        }
    }
}

/// Job statistics output.
#[derive(Debug, Serialize, Clone)]
pub struct JobStatsOutput {
    pub total: i64,
    pub pending: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
    pub cancelled: i64,
}
