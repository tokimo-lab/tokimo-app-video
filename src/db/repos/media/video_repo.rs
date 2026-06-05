use chrono::Utc;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{sea_query::Expr, *};
use uuid::Uuid;

use crate::db::entities::videos;
use crate::error::AppError;
use crate::error::OptionExt;

/// Input for updating a video category.
#[derive(Debug)]
pub struct UpdateVideoFields {
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub description: Option<String>,
    pub avatar: Option<serde_json::Value>,
    pub poster_path: Option<String>,
    pub scrape_enabled: Option<bool>,
    pub scrape_agents: Option<Vec<String>>,
    pub settings: Option<serde_json::Value>,
    pub sources: Option<serde_json::Value>,
}

pub struct VideoRepo;

impl VideoRepo {
    /// List all video categories ordered by `sort_order`.
    pub async fn list_all<C: ConnectionTrait>(db: &C) -> Result<Vec<videos::Model>, AppError> {
        let rows = videos::Entity::find()
            .order_by_asc(videos::Column::SortOrder)
            .order_by_asc(videos::Column::CreatedAt)
            .all(db)
            .await?;
        Ok(rows)
    }

    /// Get by ID.
    pub async fn get_by_id<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<Option<videos::Model>, AppError> {
        Ok(videos::Entity::find_by_id(id).one(db).await?)
    }

    /// Create a new video category.
    pub async fn create<C: ConnectionTrait>(
        db: &C,
        name: String,
        video_type: String,
        settings: Option<serde_json::Value>,
    ) -> Result<videos::Model, AppError> {
        let id = Uuid::new_v4();
        let now = Utc::now().fixed_offset();
        let max_sort = videos::Entity::find()
            .order_by_desc(videos::Column::SortOrder)
            .one(db)
            .await?
            .map_or(0, |m| m.sort_order);

        let active = videos::ActiveModel {
            id: Set(id),
            name: Set(name),
            r#type: Set(video_type),
            sort_order: Set(max_sort + 1),
            settings: Set(settings),
            sources: Set(serde_json::json!([])),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        videos::Entity::insert(active).exec(db).await?;
        videos::Entity::find_by_id(id)
            .one(db)
            .await?
            .internal("failed to fetch created video")
    }

    /// Update video fields. Only provided fields are updated.
    pub async fn update<C: ConnectionTrait>(
        db: &C,
        id: Uuid,
        input: UpdateVideoFields,
    ) -> Result<videos::Model, AppError> {
        let mut update = videos::Entity::update_many()
            .filter(videos::Column::Id.eq(id))
            .col_expr(videos::Column::UpdatedAt, Expr::value(Utc::now().fixed_offset()));

        if let Some(name) = input.name {
            update = update.col_expr(videos::Column::Name, Expr::value(name));
        }
        if let Some(t) = input.r#type {
            update = update.col_expr(videos::Column::Type, Expr::value(t));
        }
        if let Some(description) = input.description {
            update = update.col_expr(videos::Column::Description, Expr::value(Some(description)));
        }
        if let Some(avatar) = input.avatar {
            update = update.col_expr(videos::Column::Avatar, Expr::value(Some(avatar)));
        }
        if let Some(poster_path) = input.poster_path {
            update = update.col_expr(videos::Column::PosterPath, Expr::value(Some(poster_path)));
        }
        if let Some(scrape_enabled) = input.scrape_enabled {
            update = update.col_expr(videos::Column::ScrapeEnabled, Expr::value(scrape_enabled));
        }
        if let Some(scrape_agents) = input.scrape_agents {
            update = update.col_expr(videos::Column::ScrapeAgents, Expr::value(Some(scrape_agents)));
        }
        if let Some(settings) = input.settings {
            update = update.col_expr(videos::Column::Settings, Expr::value(Some(settings)));
        }
        if let Some(sources) = input.sources {
            update = update.col_expr(videos::Column::Sources, Expr::value(sources));
        }

        let results = update.exec_with_returning(db).await?;
        results.into_iter().next().not_found(format!("video {id} not found"))
    }

    /// Delete video (cascade handled by DB foreign keys).
    pub async fn delete<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<u64, AppError> {
        let result = videos::Entity::delete_by_id(id).exec(db).await?;
        Ok(result.rows_affected)
    }

    /// Batch reorder categories by setting `sort_order` for each.
    pub async fn reorder(db: &DatabaseConnection, orders: Vec<(Uuid, i32)>) -> Result<(), AppError> {
        let txn = db.begin().await?;
        for (id, sort_order) in orders {
            videos::Entity::update_many()
                .filter(videos::Column::Id.eq(id))
                .col_expr(videos::Column::SortOrder, Expr::value(sort_order))
                .exec(&txn)
                .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    /// Get sync status.
    pub async fn get_sync_status<C: ConnectionTrait>(
        db: &C,
        id: Uuid,
    ) -> Result<Option<(String, Option<DateTimeWithTimeZone>)>, AppError> {
        let model = videos::Entity::find_by_id(id).one(db).await?;
        Ok(model.map(|m| (m.sync_status, m.last_sync_at)))
    }

    /// Update sync status.
    pub async fn update_sync_status<C: ConnectionTrait>(
        db: &C,
        id: Uuid,
        status: &str,
        last_sync_at: Option<DateTimeWithTimeZone>,
    ) -> Result<(), AppError> {
        let mut update = videos::Entity::update_many()
            .filter(videos::Column::Id.eq(id))
            .col_expr(videos::Column::SyncStatus, Expr::value(status.to_string()))
            .col_expr(videos::Column::UpdatedAt, Expr::value(Utc::now().fixed_offset()));

        if let Some(ts) = last_sync_at {
            update = update.col_expr(videos::Column::LastSyncAt, Expr::value(Some(ts)));
        }

        let result = update.exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("video {id} not found")));
        }
        Ok(())
    }

    /// Parse sources JSON from the videos row.
    /// Returns `(source_id, root_path, is_default_download)` tuples.
    pub fn parse_sources(sources_json: &serde_json::Value) -> Vec<(Uuid, String, bool)> {
        sources_json
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let source_id = item
                            .get("sourceId")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse::<Uuid>().ok())?;
                        let root_path = item
                            .get("rootPath")
                            .and_then(|v| v.as_str())
                            .map(std::string::ToString::to_string)?;
                        let is_default = item
                            .get("isDefaultDownload")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        Some((source_id, root_path, is_default))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
