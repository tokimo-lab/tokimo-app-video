use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokimo_bus_client::BusClient;
use tokimo_bus_protocol::CallerCtx;
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloaderRegistration {
    pub r#type: String,
    pub display_name: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDownloadersRequest {
    pub app_id: String,
    pub downloaders: Vec<DownloaderRegistration>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDownloaderStatusRequest {
    pub record_id: Uuid,
    pub status: Option<String>,
    pub progress: Option<f64>,
    pub downloaded_bytes: Option<i64>,
    pub download_speed: Option<i64>,
    pub eta_seconds: Option<i32>,
    pub thumbnail_url: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteDownloaderRequest {
    pub record_id: Uuid,
    pub target_path: Option<String>,
    pub file_size: Option<String>,
}

pub fn video_caller() -> CallerCtx {
    CallerCtx {
        user_id: None,
        request_id: Uuid::new_v4().to_string(),
        workspace: None,
        caller_app_id: Some("video".to_string()),
    }
}

pub async fn register_downloaders(client: &BusClient) -> Result<(), AppError> {
    let request = RegisterDownloadersRequest {
        app_id: "video".to_string(),
        downloaders: vec![
            DownloaderRegistration {
                r#type: "yt-dlp".to_string(),
                display_name: "在线视频".to_string(),
                capabilities: vec!["cancel".to_string()],
            },
            DownloaderRegistration {
                r#type: "pt-qbittorrent".to_string(),
                display_name: "PT-qBittorrent".to_string(),
                capabilities: vec!["pause".to_string(), "resume".to_string(), "cancel".to_string()],
            },
        ],
    };
    invoke_json(client, "register", video_caller(), &request).await?;
    Ok(())
}

pub async fn update_status(client: &BusClient, request: &UpdateDownloaderStatusRequest) -> Result<(), AppError> {
    invoke_json(client, "update_status", video_caller(), request).await?;
    Ok(())
}

pub async fn complete(client: &BusClient, request: &CompleteDownloaderRequest) -> Result<(), AppError> {
    invoke_json(client, "complete", video_caller(), request).await?;
    Ok(())
}

async fn invoke_json<T: Serialize>(
    client: &BusClient,
    method: &str,
    caller: CallerCtx,
    request: &T,
) -> Result<Vec<u8>, AppError> {
    let payload = serde_json::to_vec(request)
        .map_err(|error| AppError::Internal(format!("downloader.{method} encode: {error}")))?;
    client
        .invoke("downloader", method, payload, caller)
        .await
        .map_err(|error| AppError::Internal(format!("downloader.{method} via bus: {error}")))
}

pub fn metadata_string(metadata: Option<&JsonValue>, key: &str) -> Option<String> {
    metadata
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .map(String::from)
}
