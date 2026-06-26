//! Person sync retry jobs for video-owned sources.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value as JsonValue, json};
use uuid::Uuid;

use crate::AppState;
use crate::bus_clients::{jobs, person};
use crate::error::AppError;
use crate::queue::cancellation::{JobCancel, check_cancel};

const RETRY_DELAY_SECS: i64 = 600;

pub async fn handle_delete_source(
    state: &Arc<AppState>,
    params: &JsonValue,
    user_id: Option<Uuid>,
    cancel: &JobCancel,
) -> Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>> {
    check_cancel(cancel)?;

    let source_app = params
        .get("sourceApp")
        .and_then(|v| v.as_str())
        .ok_or("Missing sourceApp in params")?;
    let source_id = params
        .get("sourceId")
        .and_then(|v| v.as_str())
        .ok_or("Missing sourceId in params")?;

    let bus = state.bus_client.get().ok_or("Bus client not initialized")?;
    let caller = person::video_caller(user_id);

    match person::delete_source(bus, caller, source_app, source_id).await {
        Ok(()) => {
            tracing::info!("video person_sync: deleted source {source_app}/{source_id} successfully");
            Ok(Some(json!({ "synced": true })))
        }
        Err(error) => {
            tracing::warn!(
                "video person_sync: person app unavailable for {source_app}/{source_id}, scheduling retry in {RETRY_DELAY_SECS}s: {error}"
            );
            let retry_job_id = schedule_retry(bus, user_id, source_app, source_id).await?;
            Ok(Some(json!({
                "synced": false,
                "retryScheduled": true,
                "retryJobId": retry_job_id,
            })))
        }
    }
}

async fn schedule_retry(
    bus: &tokimo_bus_client::BusClient,
    user_id: Option<Uuid>,
    source_app: &str,
    source_id: &str,
) -> Result<Uuid, AppError> {
    let user_id = user_id.ok_or_else(|| AppError::Unauthorized("person sync retry requires user_id".into()))?;
    let wake_at = (Utc::now() + chrono::Duration::seconds(RETRY_DELAY_SECS)).fixed_offset();
    let request = person::DeleteSourceJobRequest {
        source_app: source_app.to_string(),
        source_id: source_id.to_string(),
    };
    let job = jobs::create(
        bus,
        jobs::video_caller(Some(user_id)),
        jobs::CreateJobRequest::new(
            person::VIDEO_PERSON_SYNC_DELETE_SOURCE_JOB,
            serde_json::to_value(&request)?,
        )
        .with_wake_at(Some(wake_at)),
    )
    .await?;
    tracing::info!("video person_sync: scheduled retry job {} at {}", job.id, wake_at);
    Ok(job.id)
}
