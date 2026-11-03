use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use uuid::Uuid;

use crate::bus_clients::downloader::{CompleteDownloaderRequest, UpdateDownloaderStatusRequest};
use crate::db::entities::download_records;
use crate::error::AppError;

const DOWNLOADED_FILES_KEY: &str = "downloadedFiles";

/// Exact files a finished download wrote, so that deleting the record can remove
/// them without touching the shared directories they live in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedFiles {
    /// VFS source holding `paths`; `None` means `paths` are absolute local paths.
    pub source_id: Option<Uuid>,
    pub paths: Vec<String>,
}

impl DownloadedFiles {
    /// `None` for records finished before file lists were stored.
    pub fn from_record(record: &download_records::Model) -> Option<Self> {
        let value = record.app_metadata.as_ref()?.get(DOWNLOADED_FILES_KEY)?;
        serde_json::from_value(value.clone()).ok()
    }
}

#[derive(Debug, Clone)]
pub struct CreateDownloadRecordInput {
    pub id: Uuid,
    pub title: String,
    pub app_id: String,
    pub downloader_type: String,
    pub source_site: Option<String>,
    pub source_url: Option<String>,
    pub app_metadata: Option<JsonValue>,
    pub thumbnail_url: Option<String>,
    pub status: String,
    pub progress: f64,
    pub download_path: Option<String>,
}

pub struct DownloadRecordRepo;

impl DownloadRecordRepo {
    pub async fn find_online_media_duplicate<C: ConnectionTrait>(
        db: &C,
        source_url: &str,
    ) -> Result<Option<download_records::Model>, AppError> {
        Ok(download_records::Entity::find()
            .filter(download_records::Column::AppId.eq("video"))
            .filter(download_records::Column::DownloaderType.eq("yt-dlp"))
            .filter(download_records::Column::SourceUrl.eq(source_url))
            .order_by_desc(download_records::Column::CreatedAt)
            .one(db)
            .await?)
    }

    pub async fn get_model_by_id<C: ConnectionTrait>(
        db: &C,
        id: Uuid,
    ) -> Result<Option<download_records::Model>, AppError> {
        Ok(download_records::Entity::find_by_id(id).one(db).await?)
    }

    pub async fn create<C: ConnectionTrait>(
        db: &C,
        input: CreateDownloadRecordInput,
    ) -> Result<download_records::Model, AppError> {
        let now: DateTimeWithTimeZone = Utc::now().into();
        let model = download_records::ActiveModel {
            id: Set(input.id),
            title: Set(input.title),
            torrent_hash: Set(None),
            app_id: Set(input.app_id),
            downloader_type: Set(input.downloader_type),
            source_site: Set(input.source_site),
            source_url: Set(input.source_url),
            app_metadata: Set(input.app_metadata),
            download_client_id: Set(None),
            download_path: Set(input.download_path),
            target_path: Set(None),
            file_size: Set(None),
            thumbnail_url: Set(input.thumbnail_url),
            download_speed: Set(None),
            eta_seconds: Set(None),
            downloaded_bytes: Set(None),
            error_message: Set(None),
            status: Set(input.status),
            progress: Set(input.progress),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            completed_at: Set(None),
        };
        Ok(download_records::Entity::insert(model).exec_with_returning(db).await?)
    }

    /// Apply a status update to the local record. Mirrors host
    /// `downloader.update_status`; must run before pushing to the host so the
    /// sidecar's own reads (retry, PT sync, dedupe) see the same state.
    pub async fn apply_status<C: ConnectionTrait>(db: &C, req: &UpdateDownloaderStatusRequest) -> Result<(), AppError> {
        let mut active = download_records::ActiveModel {
            updated_at: Set(Some(Utc::now().into())),
            ..Default::default()
        };
        if let Some(v) = req.status.clone() {
            active.status = Set(v);
        }
        if let Some(v) = req.progress {
            active.progress = Set(v);
        }
        if let Some(v) = req.downloaded_bytes {
            active.downloaded_bytes = Set(Some(v));
        }
        if let Some(v) = req.download_speed {
            active.download_speed = Set(Some(v));
        }
        if let Some(v) = req.eta_seconds {
            active.eta_seconds = Set(Some(v));
        }
        if let Some(v) = req.thumbnail_url.clone() {
            active.thumbnail_url = Set(Some(v));
        }
        if let Some(v) = req.error_message.clone() {
            active.error_message = Set(Some(v));
        }
        download_records::Entity::update_many()
            .set(active)
            .filter(download_records::Column::Id.eq(req.record_id))
            .exec(db)
            .await?;
        Ok(())
    }

    /// Mark the local record completed. Mirrors host `downloader.complete`.
    pub async fn apply_complete<C: ConnectionTrait>(db: &C, req: &CompleteDownloaderRequest) -> Result<(), AppError> {
        let now: DateTimeWithTimeZone = Utc::now().into();
        let mut active = download_records::ActiveModel {
            status: Set("completed".to_string()),
            progress: Set(100.0),
            updated_at: Set(Some(now)),
            completed_at: Set(Some(now)),
            ..Default::default()
        };
        if let Some(v) = req.target_path.clone() {
            active.target_path = Set(Some(v));
        }
        if let Some(v) = req.file_size.clone() {
            active.file_size = Set(Some(v));
        }
        download_records::Entity::update_many()
            .set(active)
            .filter(download_records::Column::Id.eq(req.record_id))
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn record_downloaded_files<C: ConnectionTrait>(
        db: &C,
        id: Uuid,
        files: &DownloadedFiles,
    ) -> Result<(), AppError> {
        let patch = json!({ DOWNLOADED_FILES_KEY: files });
        download_records::Entity::update_many()
            .col_expr(
                download_records::Column::AppMetadata,
                Expr::cust_with_values("COALESCE(app_metadata, '{}'::jsonb) || $1::jsonb", [patch]),
            )
            .filter(download_records::Column::Id.eq(id))
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn delete<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<(), AppError> {
        download_records::Entity::delete_by_id(id).exec(db).await?;
        Ok(())
    }
}
