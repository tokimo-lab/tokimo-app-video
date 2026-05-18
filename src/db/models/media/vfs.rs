use crate::db::{ApiDateTimeExt, OptionalApiDateTimeExt};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use crate::db::entities::vfs;

/// Internal record used by `SourceRegistry` for driver management.
#[derive(Debug, Clone)]
pub struct VfsRecord {
    pub id: String,
    pub vfs_type: String,
    pub config: Value,
}

/// Public status view of a connected file system driver.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VfsStatus {
    pub id: String,
    #[ts(type = "string")]
    pub r#type: String,
    pub driver: String,
    pub state: String,
    pub error: Option<String>,
}

/// DTO for file system CRUD responses (exported to TypeScript).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VfsDto {
    pub id: String,
    pub name: String,
    #[ts(type = "string")]
    pub r#type: String,
    pub config: Option<Value>,
    pub sort_order: i32,
    pub last_scan_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<vfs::Model> for VfsDto {
    fn from(m: vfs::Model) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name,
            r#type: m.r#type,
            config: m.config.map(|v| match v {
                serde_json::Value::Null => Value::Null,
                other => other,
            }),
            sort_order: m.sort_order,
            last_scan_at: m.last_scan_at.to_api_datetime(),
            created_at: m.created_at.to_api_datetime_or_default(),
            updated_at: m.updated_at.to_api_datetime_or_default(),
        }
    }
}

/// Connection test result (exported to TypeScript).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VfsConnectionStatus {
    pub id: String,
    pub name: String,
    #[ts(type = "string")]
    pub r#type: String,
    pub is_connected: bool,
    pub error_message: Option<String>,
}
