use serde::Serialize;
use serde_json::Value as JsonValue;
use tokimo_bus_client::BusClient;
use tokimo_bus_protocol::CallerCtx;
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EmitReq {
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    payload: JsonValue,
}

pub async fn emit_entity(
    client: &BusClient,
    user_id: Uuid,
    kind: &str,
    scope: Option<String>,
    payload: JsonValue,
) -> Result<(), AppError> {
    let caller = CallerCtx {
        user_id: Some(user_id.to_string()),
        request_id: Uuid::new_v4().to_string(),
        workspace: None,
        caller_app_id: Some("video".to_string()),
    };
    let req = EmitReq {
        kind: kind.to_string(),
        scope,
        payload,
    };
    let body = serde_json::to_vec(&req)
        .map_err(|e| AppError::Internal(format!("app_events.emit encode: {e}")))?;
    client
        .invoke("app_events", "emit", body, caller)
        .await
        .map(|_| ())
        .map_err(|e| AppError::Internal(format!("app_events.emit via bus: {e}")))
}
