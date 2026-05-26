use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VideoOutput {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub avatar: Option<serde_json::Value>,
    pub description: Option<String>,
    pub poster_path: Option<String>,
    pub scrape_enabled: bool,
    pub scrape_agents: Option<Vec<String>>,
    pub sort_order: i32,
    pub settings: Option<serde_json::Value>,
    pub sync_status: String,
    pub last_sync_at: Option<String>,
    #[ts(type = "number")]
    pub item_count: i64,
    pub sources: Vec<VideoSourceOutput>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VideoSourceOutput {
    pub source_id: String,
    pub root_path: String,
    pub sort_order: i32,
    pub is_default_download: bool,
    pub source_name: Option<String>,
    pub source_type: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VideoSyncStatusOutput {
    pub video_id: String,
    pub status: String,
    pub last_sync_at: Option<String>,
}
