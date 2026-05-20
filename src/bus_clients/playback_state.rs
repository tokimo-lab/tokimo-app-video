#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokimo_bus_client::BusClient;
use tokimo_bus_protocol::CallerCtx;
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub key: Option<String>,
    pub state: JsonValue,
}

#[derive(Debug, Clone, Serialize)]
struct GetRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum GetResponse {
    Full { state: JsonValue },
    Keyed { key: String, value: JsonValue },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetRequest {
    key: String,
    state_json: JsonValue,
}

#[derive(Debug, Clone, Deserialize)]
struct SetResponse {
    ok: bool,
}

pub fn video_caller(user_id: Option<Uuid>) -> CallerCtx {
    CallerCtx {
        user_id: user_id.map(|id| id.to_string()),
        request_id: Uuid::new_v4().to_string(),
        workspace: None,
        caller_app_id: Some("video".to_string()),
    }
}

pub async fn get(client: &BusClient, caller: CallerCtx, key: Option<String>) -> Result<PlaybackState, AppError> {
    let response = invoke_json(client, "get", caller, &GetRequest { key }).await?;
    match serde_json::from_slice::<GetResponse>(&response)
        .map_err(|error| AppError::Internal(format!("playback_state.get decode: {error}")))?
    {
        GetResponse::Full { state } => Ok(PlaybackState { key: None, state }),
        GetResponse::Keyed { key, value } => Ok(PlaybackState {
            key: Some(key),
            state: value,
        }),
    }
}

pub async fn set(client: &BusClient, caller: CallerCtx, state: PlaybackState) -> Result<(), AppError> {
    let Some(key) = state.key else {
        return Err(AppError::BadRequest("playback_state.set requires key".into()));
    };
    let response = invoke_json(
        client,
        "set",
        caller,
        &SetRequest {
            key,
            state_json: state.state,
        },
    )
    .await?;
    let parsed: SetResponse = serde_json::from_slice(&response)
        .map_err(|error| AppError::Internal(format!("playback_state.set decode: {error}")))?;
    if parsed.ok {
        Ok(())
    } else {
        Err(AppError::Internal("playback_state.set returned ok=false".into()))
    }
}

async fn invoke_json<T: Serialize>(
    client: &BusClient,
    method: &str,
    caller: CallerCtx,
    request: &T,
) -> Result<Vec<u8>, AppError> {
    let payload = serde_json::to_vec(request)
        .map_err(|error| AppError::Internal(format!("playback_state.{method} encode: {error}")))?;
    client
        .invoke("playback_state", method, payload, caller)
        .await
        .map_err(|error| AppError::Internal(format!("playback_state.{method} via bus: {error}")))
}
