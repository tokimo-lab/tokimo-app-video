use chrono::Utc;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{sea_query::Expr, *};
use uuid::Uuid;

use crate::db::entities::{music_files, musics, vfs};
use crate::db::models::media::file::MediaFileStreamTarget;
use crate::error::AppError;
use crate::error::OptionExt;

#[derive(Debug)]
pub struct UpdateMusicFields {
    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar: Option<serde_json::Value>,
    pub poster_path: Option<String>,
    pub scrape_enabled: Option<bool>,
    pub scrape_agents: Option<Vec<String>>,
    pub settings: Option<serde_json::Value>,
    pub sources: Option<serde_json::Value>,
}

pub struct MusicRepo;

impl MusicRepo {
    pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<musics::Model>, AppError> {
        let rows = musics::Entity::find()
            .order_by_asc(musics::Column::SortOrder)
            .order_by_asc(musics::Column::CreatedAt)
            .all(db)
            .await?;
        Ok(rows)
    }

    pub async fn get_by_id(db: &DatabaseConnection, id: Uuid) -> Result<Option<musics::Model>, AppError> {
        Ok(musics::Entity::find_by_id(id).one(db).await?)
    }

    pub async fn create(
        db: &DatabaseConnection,
        name: String,
        music_type: String,
        settings: Option<serde_json::Value>,
    ) -> Result<musics::Model, AppError> {
        let id = Uuid::new_v4();
        let now = Utc::now().fixed_offset();
        let max_sort = musics::Entity::find()
            .order_by_desc(musics::Column::SortOrder)
            .one(db)
            .await?
            .map_or(0, |m| m.sort_order);

        let active = musics::ActiveModel {
            id: Set(id),
            name: Set(name),
            r#type: Set(music_type),
            sort_order: Set(max_sort + 1),
            settings: Set(settings),
            sources: Set(serde_json::json!([])),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        musics::Entity::insert(active).exec(db).await?;
        musics::Entity::find_by_id(id)
            .one(db)
            .await?
            .internal("failed to fetch created music library")
    }

    pub async fn update(
        db: &DatabaseConnection,
        id: Uuid,
        input: UpdateMusicFields,
    ) -> Result<musics::Model, AppError> {
        let model = musics::Entity::find_by_id(id)
            .one(db)
            .await?
            .not_found(format!("music library {id} not found"))?;
        let mut active: musics::ActiveModel = model.into();
        if let Some(name) = input.name {
            active.name = Set(name);
        }
        if let Some(description) = input.description {
            active.description = Set(Some(description));
        }
        if let Some(avatar) = input.avatar {
            active.avatar = Set(Some(avatar));
        }
        if let Some(poster_path) = input.poster_path {
            active.poster_path = Set(Some(poster_path));
        }
        if let Some(scrape_enabled) = input.scrape_enabled {
            active.scrape_enabled = Set(scrape_enabled);
        }
        if let Some(scrape_agents) = input.scrape_agents {
            active.scrape_agents = Set(Some(scrape_agents));
        }
        if let Some(settings) = input.settings {
            active.settings = Set(Some(settings));
        }
        if let Some(sources) = input.sources {
            active.sources = Set(sources);
        }
        active.updated_at = Set(Some(Utc::now().fixed_offset()));
        let updated = active.update(db).await?;
        Ok(updated)
    }

    pub async fn delete(db: &DatabaseConnection, id: Uuid) -> Result<u64, AppError> {
        let result = musics::Entity::delete_by_id(id).exec(db).await?;
        Ok(result.rows_affected)
    }

    pub async fn reorder(db: &DatabaseConnection, orders: Vec<(Uuid, i32)>) -> Result<(), AppError> {
        for (id, sort_order) in orders {
            musics::Entity::update_many()
                .filter(musics::Column::Id.eq(id))
                .col_expr(musics::Column::SortOrder, Expr::value(sort_order))
                .exec(db)
                .await?;
        }
        Ok(())
    }

    pub async fn get_sync_status(
        db: &DatabaseConnection,
        id: Uuid,
    ) -> Result<Option<(String, Option<DateTimeWithTimeZone>)>, AppError> {
        let model = musics::Entity::find_by_id(id).one(db).await?;
        Ok(model.map(|m| (m.sync_status, m.last_sync_at)))
    }

    pub async fn update_sync_status(
        db: &DatabaseConnection,
        id: Uuid,
        status: &str,
        last_sync_at: Option<DateTimeWithTimeZone>,
    ) -> Result<(), AppError> {
        let model = musics::Entity::find_by_id(id)
            .one(db)
            .await?
            .not_found(format!("music library {id} not found"))?;
        let mut active: musics::ActiveModel = model.into();
        active.sync_status = Set(status.to_string());
        if let Some(ts) = last_sync_at {
            active.last_sync_at = Set(Some(ts));
        }
        active.updated_at = Set(Some(Utc::now().fixed_offset()));
        active.update(db).await?;
        Ok(())
    }

    /// Parse sources JSON. Returns `(source_id, root_path, is_default_download)` tuples.
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

    /// Load minimal stream info for a music file (path + source_id).
    pub async fn load_stream_target(
        db: &DatabaseConnection,
        file_id: &str,
    ) -> Result<Option<MediaFileStreamTarget>, AppError> {
        let fid: Uuid = file_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid file id".into()))?;
        let row = music_files::Entity::find_by_id(fid)
            .find_also_related(vfs::Entity)
            .one(db)
            .await?;

        Ok(row.map(|(mf, fs)| {
            let (source_type, source_config) = match fs {
                Some(s) => (Some(s.r#type), s.config),
                None => (None, None),
            };
            MediaFileStreamTarget {
                path: mf.path,
                source_id: mf.source_id.map(|id| id.to_string()),
                source_type,
                source_config,
                video_item_id: None,
                episode_id: None,
                duration: mf.duration.map(f64::from),
                size: mf.size,
            }
        }))
    }
}
