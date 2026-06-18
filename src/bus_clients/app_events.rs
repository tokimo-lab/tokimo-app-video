use serde::Serialize;
use serde_json::Value as JsonValue;
use tokimo_bus_client::BusClient;

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
    kind: &str,
    scope: Option<String>,
    payload: JsonValue,
) -> Result<(), AppError> {
    let caller = client.auto_caller("video");
    let req = EmitReq {
        kind: kind.to_string(),
        scope,
        payload,
    };
    let body = serde_json::to_vec(&req).map_err(|e| AppError::Internal(format!("app_events.emit encode: {e}")))?;
    client
        .invoke("app_events", "emit", body, caller)
        .await
        .map(|_| ())
        .map_err(|e| AppError::Internal(format!("app_events.emit via bus: {e}")))
}
