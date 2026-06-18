#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokimo_bus_client::BusClient;
use tokimo_bus_protocol::CallerCtx;
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDriverConfigRequest {
    pub source_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverConfig {
    pub driver_name: String,
    pub config: JsonValue,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchConfigRequest {
    pub source_id: Uuid,
    pub patch: JsonValue,
}

pub async fn get_driver_config(
    client: &BusClient,
    source_id: Uuid,
) -> Result<DriverConfig, AppError> {
    let response = invoke_json(
        client,
        "get_driver_config",
        client.auto_caller("video"),
        &GetDriverConfigRequest { source_id },
    )
    .await?;
    serde_json::from_slice(&response)
        .map_err(|error| AppError::Internal(format!("vfs.get_driver_config decode: {error}")))
}

pub async fn patch_config(
    client: &BusClient,
    source_id: Uuid,
    patch: JsonValue,
) -> Result<(), AppError> {
    let _ = invoke_json(client, "patch_config", client.auto_caller("video"), &PatchConfigRequest { source_id, patch }).await?;
    Ok(())
}

async fn invoke_json<T: Serialize>(
    client: &BusClient,
    method: &str,
    caller: CallerCtx,
    request: &T,
) -> Result<Vec<u8>, AppError> {
    let payload =
        serde_json::to_vec(request).map_err(|error| AppError::Internal(format!("vfs.{method} encode: {error}")))?;
    client
        .invoke("vfs", method, payload, caller)
        .await
        .map_err(|error| AppError::Internal(format!("vfs.{method} via bus: {error}")))
}
