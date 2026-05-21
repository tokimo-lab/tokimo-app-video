use chrono::Utc;
use sea_orm::*;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::db::entities::organize_reports;
use crate::error::AppError;
use crate::error::OptionExt;

/// Input for creating an organize report.
#[derive(Debug)]
pub struct CreateOrganizeReportInput {
    pub source_path: String,
    pub total_items: String,
    pub success_count: String,
    pub failed_count: String,
    pub skipped_count: String,
    pub results: JsonValue,
    pub media_names: JsonValue,
}

pub struct OrganizeReportRepo;

impl OrganizeReportRepo {
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<organize_reports::Model>, AppError> {
        let models = organize_reports::Entity::find()
            .order_by_desc(organize_reports::Column::CreatedAt)
            .all(db)
            .await?;
        Ok(models)
    }

    pub async fn get_by_id(db: &DatabaseConnection, id: Uuid) -> Result<Option<organize_reports::Model>, AppError> {
        Ok(organize_reports::Entity::find_by_id(id).one(db).await?)
    }

    pub async fn create(
        db: &DatabaseConnection,
        input: CreateOrganizeReportInput,
    ) -> Result<organize_reports::Model, AppError> {
        let id = Uuid::new_v4();
        let now = Utc::now().fixed_offset();
        let model = organize_reports::ActiveModel {
            id: Set(id),
            source_path: Set(input.source_path),
            total_items: Set(input.total_items),
            success_count: Set(input.success_count),
            failed_count: Set(input.failed_count),
            skipped_count: Set(input.skipped_count),
            results: Set(input.results),
            media_names: Set(input.media_names),
            created_at: Set(Some(now)),
        };
        organize_reports::Entity::insert(model).exec(db).await?;
        organize_reports::Entity::find_by_id(id)
            .one(db)
            .await?
            .not_found("failed to fetch created organize report")
    }

    pub async fn delete(db: &DatabaseConnection, id: Uuid) -> Result<bool, AppError> {
        let result = organize_reports::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("organize report {id} not found")));
        }
        Ok(true)
    }
}
