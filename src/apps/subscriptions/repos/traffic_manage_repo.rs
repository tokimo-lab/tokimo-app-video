use crate::db::OptionalApiDateTimeExt;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::db::entities::{download_records, pt_sites};
use crate::db::repos::system_config_repo::{SystemConfigRepo, SystemConfigSection};
use crate::error::AppError;

// ── TrafficManageSettings ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficManageSettings {
    pub download_path: String,
    pub min_free_disk_space_gb: i32,
    pub stats_window_minutes: i32,
    pub max_upload_rate_mbps: i32,
    pub max_active_torrents: i32,
    pub scan_interval_minutes: i32,
    pub cleanup_interval_minutes: i32,
    pub download_client_id: Option<String>,
    pub is_enabled: bool,
}

impl SystemConfigSection for TrafficManageSettings {
    const SCOPE: &'static str = "download";
    const SCOPE_ID: &'static str = "traffic_manage";
    fn default_value() -> Self {
        Self {
            download_path: "/downloads/traffic".to_string(),
            min_free_disk_space_gb: 20,
            stats_window_minutes: 60,
            max_upload_rate_mbps: 0,
            max_active_torrents: 10,
            scan_interval_minutes: 30,
            cleanup_interval_minutes: 60,
            download_client_id: None,
            is_enabled: false,
        }
    }
}

// ── DTOs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrafficManageSettingsDto {
    pub download_path: String,
    pub min_free_disk_space_gb: i32,
    pub stats_window_minutes: i32,
    pub max_upload_rate_mbps: i32,
    pub max_active_torrents: i32,
    pub scan_interval_minutes: i32,
    pub cleanup_interval_minutes: i32,
    pub download_client_id: Option<String>,
    pub is_enabled: bool,
}

impl From<TrafficManageSettings> for TrafficManageSettingsDto {
    fn from(s: TrafficManageSettings) -> Self {
        Self {
            download_path: s.download_path,
            min_free_disk_space_gb: s.min_free_disk_space_gb,
            stats_window_minutes: s.stats_window_minutes,
            max_upload_rate_mbps: s.max_upload_rate_mbps,
            max_active_torrents: s.max_active_torrents,
            scan_interval_minutes: s.scan_interval_minutes,
            cleanup_interval_minutes: s.cleanup_interval_minutes,
            download_client_id: s.download_client_id,
            is_enabled: s.is_enabled,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrafficManageLogDto {
    pub id: String,
    pub pt_site_id: Option<String>,
    pub pt_site_name: Option<String>,
    pub torrent_name: String,
    pub file_size: Option<String>,
    /// V4 schema: downloaded_bytes stored as string for TS compatibility
    pub downloaded_size: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrafficManageStatsDto {
    pub total_downloaded: String,
    pub active_torrents: i64,
    pub total_torrents: i64,
}

// ── Input ─────────────────────────────────────────────────────────────────────

pub struct UpdateTrafficSettingsInput {
    pub download_path: Option<String>,
    pub min_free_disk_space_gb: Option<i32>,
    pub stats_window_minutes: Option<i32>,
    pub max_upload_rate_mbps: Option<i32>,
    pub max_active_torrents: Option<i32>,
    pub scan_interval_minutes: Option<i32>,
    pub cleanup_interval_minutes: Option<i32>,
    pub download_client_id: Option<Option<String>>,
    pub is_enabled: Option<bool>,
}

// ── Repo ──────────────────────────────────────────────────────────────────────

/// V4 schema: PT traffic-managed records are identified by `downloader_type = "pt-qbittorrent"`.
/// pt_site_id is stored in `app_metadata->>'ptSiteId'`.
const PT_DOWNLOADER_TYPE: &str = "pt-qbittorrent";

pub struct TrafficManageRepo;

impl TrafficManageRepo {
    pub async fn get_settings<C: ConnectionTrait>(db: &C) -> Result<TrafficManageSettings, AppError> {
        SystemConfigRepo::get::<TrafficManageSettings>(db).await
    }

    pub async fn upsert_settings<C: ConnectionTrait>(
        db: &C,
        input: UpdateTrafficSettingsInput,
    ) -> Result<TrafficManageSettings, AppError> {
        let mut settings = SystemConfigRepo::get::<TrafficManageSettings>(db).await?;

        if let Some(v) = input.download_path {
            settings.download_path = v;
        }
        if let Some(v) = input.min_free_disk_space_gb {
            settings.min_free_disk_space_gb = v;
        }
        if let Some(v) = input.stats_window_minutes {
            settings.stats_window_minutes = v;
        }
        if let Some(v) = input.max_upload_rate_mbps {
            settings.max_upload_rate_mbps = v;
        }
        if let Some(v) = input.max_active_torrents {
            settings.max_active_torrents = v;
        }
        if let Some(v) = input.scan_interval_minutes {
            settings.scan_interval_minutes = v;
        }
        if let Some(v) = input.cleanup_interval_minutes {
            settings.cleanup_interval_minutes = v;
        }
        if let Some(v) = input.download_client_id {
            settings.download_client_id = v;
        }
        if let Some(v) = input.is_enabled {
            settings.is_enabled = v;
        }

        SystemConfigRepo::set(db, &settings).await?;
        Ok(settings)
    }

    pub async fn get_logs<C: ConnectionTrait>(
        db: &C,
        pt_site_id: Option<Uuid>,
        limit: u64,
        offset: u64,
    ) -> Result<(Vec<TrafficManageLogDto>, u64), AppError> {
        let rows = download_records::Entity::find()
            .filter(download_records::Column::DownloaderType.eq(PT_DOWNLOADER_TYPE))
            .order_by_desc(download_records::Column::CreatedAt)
            .all(db)
            .await?;

        // Collect pt_site UUIDs for name lookup
        let mut pt_ids = std::collections::HashSet::new();
        for row in &rows {
            if let Some(id) = pt_site_id_from_meta(row.app_metadata.as_ref()) {
                pt_ids.insert(id);
            }
        }
        let mut pt_name_map = std::collections::HashMap::new();
        if !pt_ids.is_empty() {
            let sites = pt_sites::Entity::find()
                .filter(pt_sites::Column::Id.is_in(pt_ids))
                .all(db)
                .await?;
            for site in sites {
                pt_name_map.insert(site.id, site.name);
            }
        }

        let filtered: Vec<_> = rows
            .into_iter()
            .filter(|record| match pt_site_id {
                Some(expected) => pt_site_id_from_meta(record.app_metadata.as_ref()) == Some(expected),
                None => true,
            })
            .collect();
        let total = filtered.len() as u64;

        let items = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|record| {
                let pt_id = pt_site_id_from_meta(record.app_metadata.as_ref());
                TrafficManageLogDto {
                    id: record.id.to_string(),
                    pt_site_id: pt_id.map(|id| id.to_string()),
                    pt_site_name: pt_id.and_then(|id| pt_name_map.get(&id).cloned()),
                    torrent_name: record.title,
                    file_size: record.file_size,
                    downloaded_size: record.downloaded_bytes.map(|b| b.to_string()),
                    status: record.status,
                    created_at: record.created_at.to_api_datetime_or_default(),
                    updated_at: record.updated_at.to_api_datetime_or_default(),
                }
            })
            .collect();

        Ok((items, total))
    }

    pub async fn get_stats<C: ConnectionTrait>(db: &C) -> Result<TrafficManageStatsDto, AppError> {
        let records = download_records::Entity::find()
            .filter(download_records::Column::DownloaderType.eq(PT_DOWNLOADER_TYPE))
            .all(db)
            .await?;

        let mut total_downloaded: u64 = 0;
        let mut active_torrents: i64 = 0;
        let total_torrents = records.len() as i64;

        for record in &records {
            if let Some(bytes) = record.downloaded_bytes {
                total_downloaded += bytes as u64;
            }
            if record.status == "downloading" || record.status == "seeding" {
                active_torrents += 1;
            }
        }

        Ok(TrafficManageStatsDto {
            total_downloaded: total_downloaded.to_string(),
            active_torrents,
            total_torrents,
        })
    }
}

fn pt_site_id_from_meta(meta: Option<&Value>) -> Option<Uuid> {
    meta.and_then(Value::as_object)
        .and_then(|obj| obj.get("ptSiteId"))
        .and_then(Value::as_str)
        .and_then(|id| Uuid::parse_str(id).ok())
}
