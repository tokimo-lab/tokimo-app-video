use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Serialize, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PtSiteDto {
    pub id: String,
    pub name: String,
    pub site_id: String,
    pub domain: String,
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookies: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | undefined")]
    pub auto_stop_minutes: Option<i64>,
    pub traffic_manage_enabled: bool,
    pub traffic_manage_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traffic_manage_target: Option<String>,
    pub adult_enabled: bool,
    pub sort_order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AvailableSiteDto {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub allow_auth_type: Vec<String>,
    pub has_adult_content: bool,
    pub adult_only: bool,
}

#[derive(Debug, Serialize, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PtUserInfoDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploaded: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | undefined")]
    pub seeding: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | undefined")]
    pub leeching: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vip_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bonus: Option<String>,
}

impl PtUserInfoDto {
    pub fn empty() -> Self {
        Self {
            uid: None,
            username: None,
            uploaded: None,
            downloaded: None,
            share_ratio: None,
            seeding: None,
            leeching: None,
            vip_group: None,
            bonus: None,
        }
    }
}

#[derive(Debug, Serialize, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PtSiteStatusDto {
    pub id: String,
    pub name: String,
    pub site_id: String,
    pub is_logged_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_info: Option<PtUserInfoDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}
