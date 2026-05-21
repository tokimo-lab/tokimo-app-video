use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::db::entities::download_records;
use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct CreateDownloadRecordInput {
    pub id: Uuid,
    pub torrent_name: String,
    pub source_origin: String,
    pub source_site: Option<String>,
    pub source_url: Option<String>,
    pub content_type: String,
    pub media_title: Option<String>,
    pub media_year: Option<String>,
    pub thumbnail_url: Option<String>,
    pub duration_seconds: Option<i32>,
    pub uploader: Option<String>,
    pub external_id: Option<String>,
    pub status: String,
    pub progress: Option<String>,
    pub analysis_snapshot: Option<JsonValue>,
    pub auto_organize: bool,
    pub import_status: Option<String>,
    pub download_path: Option<String>,
    pub target_video_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
}

pub struct DownloadRecordRepo;

impl DownloadRecordRepo {
    pub async fn find_online_media_duplicate<C: ConnectionTrait>(
        db: &C,
        source_url: &str,
    ) -> Result<Option<download_records::Model>, AppError> {
        Ok(download_records::Entity::find()
            .filter(download_records::Column::SourceOrigin.eq("online_media"))
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
            torrent_name: Set(input.torrent_name),
            torrent_hash: Set(None),
            source_origin: Set(input.source_origin),
            source_site: Set(input.source_site),
            source_url: Set(input.source_url),
            content_type: Set(input.content_type),
            media_title: Set(input.media_title),
            media_year: Set(input.media_year),
            tmdb_id: Set(None),
            imdb_id: Set(None),
            season: Set(None),
            episode: Set(None),
            episodes: Set(None),
            quality: Set(None),
            source: Set(None),
            codec: Set(None),
            release_group: Set(None),
            pt_site_id: Set(None),
            download_client_id: Set(None),
            download_path: Set(input.download_path),
            target_path: Set(None),
            file_size: Set(None),
            thumbnail_url: Set(input.thumbnail_url),
            duration_seconds: Set(input.duration_seconds),
            uploader: Set(input.uploader),
            external_id: Set(input.external_id),
            status: Set(input.status),
            progress: Set(input.progress),
            uploaded_size: Set(None),
            downloaded_size: Set(None),
            ratio: Set(None),
            seeding_time: Set(None),
            is_recognized: Set(true),
            analysis_snapshot: Set(input.analysis_snapshot),
            manifest_path: Set(None),
            rust_task_id: Set(None),
            import_status: Set(input.import_status),
            import_error: Set(None),
            subscription_id: Set(None),
            target_video_id: Set(input.target_video_id),
            link_mode: Set(None),
            poster_path: Set(None),
            auto_organize: Set(input.auto_organize),
            is_traffic_manage: Set(false),
            auto_stop_at: Set(None),
            music_album_id: Set(None),
            created_by: Set(input.created_by),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
        };
        Ok(download_records::Entity::insert(model).exec_with_returning(db).await?)
    }

    pub async fn delete<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<(), AppError> {
        download_records::Entity::delete_by_id(id).exec(db).await?;
        Ok(())
    }

    pub async fn reset_for_retry<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<(), AppError> {
        let now: DateTimeWithTimeZone = Utc::now().into();
        let result = download_records::Entity::update_many()
            .col_expr(download_records::Column::Status, Expr::value("downloading"))
            .col_expr(download_records::Column::Progress, Expr::value(Some("0".to_string())))
            .col_expr(
                download_records::Column::RustTaskId,
                Expr::value::<Option<String>>(None),
            )
            .col_expr(
                download_records::Column::ImportStatus,
                Expr::value(Some("pending".to_string())),
            )
            .col_expr(
                download_records::Column::ImportError,
                Expr::value::<Option<String>>(None),
            )
            .col_expr(download_records::Column::UpdatedAt, Expr::value(Some(now)))
            .filter(download_records::Column::Id.eq(id))
            .exec(db)
            .await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound("下载记录不存在".into()));
        }
        Ok(())
    }

    pub async fn mark_job_creation_failed<C: ConnectionTrait>(db: &C, id: Uuid, message: &str) -> Result<(), AppError> {
        let now: DateTimeWithTimeZone = Utc::now().into();
        let result = download_records::Entity::update_many()
            .col_expr(download_records::Column::Status, Expr::value("failed"))
            .col_expr(
                download_records::Column::ImportStatus,
                Expr::value(Some("failed".to_string())),
            )
            .col_expr(
                download_records::Column::ImportError,
                Expr::value(Some(message.to_string())),
            )
            .col_expr(
                download_records::Column::RustTaskId,
                Expr::value::<Option<String>>(None),
            )
            .col_expr(download_records::Column::UpdatedAt, Expr::value(Some(now)))
            .filter(download_records::Column::Id.eq(id))
            .exec(db)
            .await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound("下载记录不存在".into()));
        }
        Ok(())
    }
}
