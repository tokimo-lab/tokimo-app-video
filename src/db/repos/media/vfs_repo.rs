use sea_orm::{sea_query::Expr, *};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::db::entities::vfs;
use crate::db::models::media::vfs::VfsRecord;
use crate::error::AppError;
use crate::error::OptionExt;

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

    /// Create a new file system.
    pub async fn create(
        db: &DatabaseConnection,
        name: String,
        fs_type: String,
        config: Option<serde_json::Value>,
    ) -> Result<vfs::Model, AppError> {
        let id = Uuid::new_v4();
        let max_sort = vfs::Entity::find()
            .order_by_desc(vfs::Column::SortOrder)
            .one(db)
            .await?
            .map_or(0, |m| m.sort_order);

        let active = vfs::ActiveModel {
            id: Set(id),
            name: Set(name),
            r#type: Set(fs_type),
            config: Set(config),
            sort_order: Set(max_sort + 1),
            ..Default::default()
        };
        vfs::Entity::insert(active).exec(db).await?;
        // Re-fetch to get server defaults (created_at, updated_at)
        vfs::Entity::find_by_id(id)
            .one(db)
            .await?
            .internal("failed to fetch created vfs")
    }

    /// Update a file system. Only provided fields are updated.
    /// When `config` is provided, its keys are *merged* into the existing
    /// config rather than replacing it entirely.  This preserves runtime-
    /// managed credentials (e.g. Baidu `refresh_token`) that are not exposed
    /// in the settings UI.
    pub async fn update(
        db: &DatabaseConnection,
        id: Uuid,
        name: Option<String>,
        fs_type: Option<String>,
        config: Option<Option<serde_json::Value>>,
    ) -> Result<vfs::Model, AppError> {
        let model = vfs::Entity::find_by_id(id)
            .one(db)
            .await?
            .not_found(format!("vfs {id} not found"))?;
        let mut active: vfs::ActiveModel = model.clone().into();
        if let Some(name) = name {
            active.name = Set(name);
        }
        if let Some(fs_type) = fs_type {
            active.r#type = Set(fs_type);
        }
        if let Some(new_config) = config {
            let merged = match (
                model.config.as_ref().and_then(|c| c.as_object()),
                new_config.as_ref().and_then(|c| c.as_object()),
            ) {
                (Some(existing), Some(incoming)) => {
                    let mut base = existing.clone();
                    for (k, v) in incoming {
                        base.insert(k.clone(), v.clone());
                    }
                    Some(Value::Object(base))
                }
                // If either side isn't an object, fall back to full replace.
                _ => new_config,
            };
            active.config = Set(merged);
        }
        let updated = active.update(db).await?;
        Ok(updated)
    }

    /// Delete a file system by ID. Returns true if a row was deleted.
    pub async fn delete(db: &DatabaseConnection, id: Uuid) -> Result<bool, AppError> {
        let result = vfs::Entity::delete_by_id(id).exec(db).await?;
        Ok(result.rows_affected > 0)
    }

    /// Merge a JSON patch into an existing file system's config.
    /// Used by `SourceRegistry` to persist credentials obtained at runtime
    /// (e.g. cookie from QR code login).
    pub async fn patch_config(db: &DatabaseConnection, id: &str, patch: serde_json::Value) -> Result<(), AppError> {
        let uid: Uuid = id.parse().map_err(|_| AppError::BadRequest("invalid vfs id".into()))?;
        let model = vfs::Entity::find_by_id(uid)
            .one(db)
            .await?
            .not_found(format!("vfs {id} not found"))?;
        let mut config = model.config.clone().unwrap_or_else(|| Value::Object(Map::new()));
        if let (Some(base), Some(patch_obj)) = (config.as_object_mut(), patch.as_object()) {
            for (k, v) in patch_obj {
                base.insert(k.clone(), v.clone());
            }
        }
        let mut active: vfs::ActiveModel = model.into();
        active.config = Set(Some(config));
        active.update(db).await?;
        Ok(())
    }

    /// Reorder file systems by setting `sort_order` for each.
    pub async fn reorder(db: &DatabaseConnection, orders: Vec<(Uuid, i32)>) -> Result<(), AppError> {
        for (id, sort_order) in orders {
            vfs::Entity::update_many()
                .filter(vfs::Column::Id.eq(id))
                .col_expr(vfs::Column::SortOrder, Expr::value(sort_order))
                .exec(db)
                .await?;
        }
        Ok(())
    }
}
