#![allow(dead_code)]

use serde::Serialize;
use tokimo_bus_client::BusClient;
use tokimo_bus_protocol::CallerCtx;
use uuid::Uuid;

use crate::bus_clients::jobs::{self, CreateJobRequest};
use crate::error::AppError;

pub const VIDEO_PERSON_SYNC_DELETE_SOURCE_JOB: &str = "video_person_sync_delete_source";

#[derive(Debug, Clone, Serialize)]
pub struct DeleteSourceRequest {
    pub source_app: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSourceJobRequest {
    pub source_app: String,
    pub source_id: String,
}

pub fn video_caller(user_id: Option<Uuid>) -> CallerCtx {
    CallerCtx {
        user_id: user_id.map(|id| id.to_string()),
        request_id: Uuid::new_v4().to_string(),
        workspace: None,
        caller_app_id: Some("video".to_string()),
    }
}

pub async fn delete_source(
    client: &BusClient,
    caller: CallerCtx,
    source_app: &str,
    source_id: &str,
) -> Result<(), AppError> {
    let request = DeleteSourceRequest {
        source_app: source_app.to_string(),
        source_id: source_id.to_string(),
    };
    let _ = invoke_json(client, "delete_source", caller, &request).await?;
    Ok(())
}

pub async fn delete_source_via_job(
    client: &BusClient,
    caller: CallerCtx,
    source_app: &str,
    source_id: &str,
) -> Result<Uuid, AppError> {
    let request = DeleteSourceJobRequest {
        source_app: source_app.to_string(),
        source_id: source_id.to_string(),
    };
    let job = jobs::create(
        client,
        caller,
        CreateJobRequest::new(VIDEO_PERSON_SYNC_DELETE_SOURCE_JOB, serde_json::to_value(&request)?),
    )
    .await?;
    Ok(job.id)
}

async fn invoke_json<T: Serialize>(
    client: &BusClient,
    method: &str,
    caller: CallerCtx,
    request: &T,
) -> Result<Vec<u8>, AppError> {
    let payload =
        serde_json::to_vec(request).map_err(|error| AppError::Internal(format!("person.{method} encode: {error}")))?;
    client
        .invoke("person", method, payload, caller)
        .await
        .map_err(|error| AppError::Internal(format!("person.{method} via bus: {error}")))
}
