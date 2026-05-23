use sea_orm::*;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::db::entities::vfs;
use crate::db::models::media::vfs::VfsRecord;
use crate::error::AppError;

pub struct VfsRepo;

impl VfsRepo {
    /// Fetch all file systems as lightweight records (for `SourceRegistry`).
    pub async fn fetch_all(db: &DatabaseConnection) -> Result<Vec<VfsRecord>, AppError> {
        let rows = vfs::Entity::find().all(db).await?;
        Ok(rows
            .into_iter()
            .map(|r| VfsRecord {
                id: r.id.to_string(),
                vfs_type: r.r#type,
                config: r.config.unwrap_or_else(|| Value::Object(Map::new())),
            })
            .collect())
    }

    /// Fetch a single file system record by ID (for `SourceRegistry`).
    pub async fn fetch_by_id(db: &DatabaseConnection, id: &str) -> Result<Option<VfsRecord>, AppError> {
        let uid: Uuid = id.parse().map_err(|_| AppError::BadRequest("invalid vfs id".into()))?;
        let row = vfs::Entity::find_by_id(uid).one(db).await?;
        Ok(row.map(|r| VfsRecord {
            id: r.id.to_string(),
            vfs_type: r.r#type,
            config: r.config.unwrap_or_else(|| Value::Object(Map::new())),
        }))
    }

    /// List all file systems ordered by `sort_order` then `created_at`.
    pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<vfs::Model>, AppError> {
        let rows = vfs::Entity::find()
            .order_by_asc(vfs::Column::SortOrder)
            .order_by_asc(vfs::Column::CreatedAt)
            .all(db)
            .await?;
        Ok(rows)
    }

    /// Get a single file system entity by ID.
    pub async fn get_by_id(db: &DatabaseConnection, id: Uuid) -> Result<Option<vfs::Model>, AppError> {
        Ok(vfs::Entity::find_by_id(id).one(db).await?)
    }
}
