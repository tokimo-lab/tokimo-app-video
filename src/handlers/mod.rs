pub mod crud;
pub mod browse;
pub mod sync;
pub mod subtitle;
pub mod subtitle_events;
pub mod playback;
pub mod playback_state;
pub mod hls;
pub mod file_stream;
pub mod iso_reader;
pub(super) mod udfread_ffi;

use serde::Deserialize;
use uuid::Uuid;

use crate::db::entities::vfs;
use crate::db::models::video::{VideoOutput, VideoSourceOutput};
use crate::db::repos::media::VideoRepo;
use crate::db::{ApiDateTimeExt, OptionalApiDateTimeExt};
use crate::error::AppError;

pub use crud::*;
pub use browse::*;
pub use sync::*;

// ── Input DTOs ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVideoInput {
    pub name: String,
    pub r#type: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub scrape_enabled: Option<bool>,
    pub scrape_agents: Option<Vec<String>>,
    pub settings: Option<serde_json::Value>,
    pub sources: Option<Vec<VideoSourceInput>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateVideoInput {
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub scrape_enabled: Option<bool>,
    pub scrape_agents: Option<Vec<String>>,
    pub settings: Option<serde_json::Value>,
    pub sources: Option<Vec<VideoSourceInput>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSourceInput {
    pub source_id: String,
    pub root_path: String,
    pub sort_order: i32,
    pub is_default_download: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSyncInput {
    pub clear_data: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoReorderInput {
    pub orders: Vec<VideoReorderItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoReorderItem {
    pub id: String,
    pub sort_order: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoListMediaQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub genre_id: Option<String>,
    pub search: Option<String>,
    pub country: Option<String>,
    pub favorite: Option<bool>,
    pub resolution: Option<String>,
    pub runtime: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoRecentlyAddedQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoCollectionsQuery {
    pub video_item_id: Option<String>,
    pub tv_show_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoToggleFavoriteInput {
    pub r#type: String,
    pub id: String,
}

// ── Shared helpers ──

pub(crate) fn parse_uuid(s: &str) -> Result<Uuid, AppError> {
    s.parse::<Uuid>()
        .map_err(|_| AppError::BadRequest(format!("invalid uuid: {s}")))
}

/// Build sources JSON from input.
pub(crate) fn sources_to_json(sources: &[VideoSourceInput]) -> serde_json::Value {
    serde_json::json!(
        sources
            .iter()
            .enumerate()
            .map(|(i, s)| {
                serde_json::json!({
                    "sourceId": s.source_id,
                    "rootPath": s.root_path,
                    "sortOrder": s.sort_order.max(i as i32),
                    "isDefaultDownload": s.is_default_download.unwrap_or(false),
                })
            })
            .collect::<Vec<_>>()
    )
}

/// Convert a `videos::Model` into a `VideoOutput` DTO.
pub(crate) async fn to_video_output(
    db: &sea_orm::DatabaseConnection,
    model: crate::db::entities::videos::Model,
) -> Result<VideoOutput, AppError> {
    use sea_orm::EntityTrait;

    let video_id = model.id;

    // Parse sources JSON and enrich with file system info
    let source_tuples = VideoRepo::parse_sources(&model.sources);
    let mut sources = Vec::with_capacity(source_tuples.len());
    for (source_id, root_path, is_default_download) in &source_tuples {
        let fs = vfs::Entity::find_by_id(*source_id).one(db).await?;
        sources.push(VideoSourceOutput {
            source_id: source_id.to_string(),
            root_path: root_path.clone(),
            sort_order: sources.len() as i32,
            is_default_download: *is_default_download,
            source_name: fs.as_ref().map(|f| f.name.clone()),
            source_type: fs.as_ref().map(|f| f.r#type.clone()),
        });
    }

    // Count items (movies + tv shows)
    use crate::db::entities::{video_items, tv_shows};
    use sea_orm::*;
    let video_item_count = video_items::Entity::find()
        .filter(video_items::Column::VideoId.eq(video_id))
        .count(db)
        .await? as i64;
    let tv_count = tv_shows::Entity::find()
        .filter(tv_shows::Column::VideoId.eq(video_id))
        .count(db)
        .await? as i64;

    Ok(VideoOutput {
        id: model.id.to_string(),
        name: model.name,
        r#type: model.r#type,
        icon: model.icon,
        color: model.color,
        description: model.description,
        poster_path: model.poster_path,
        scrape_enabled: model.scrape_enabled,
        scrape_agents: model.scrape_agents,
        sort_order: model.sort_order,
        settings: model.settings,
        sync_status: model.sync_status,
        last_sync_at: model.last_sync_at.to_api_datetime(),
        item_count: video_item_count + tv_count,
        sources,
        created_at: model.created_at.to_api_datetime_or_default(),
        updated_at: model.updated_at.to_api_datetime_or_default(),
    })
}

/// Build `VideoOutput` for a list of models.
pub(crate) async fn to_video_outputs(
    db: &sea_orm::DatabaseConnection,
    models: Vec<crate::db::entities::videos::Model>,
) -> Result<Vec<VideoOutput>, AppError> {
    let mut outputs = Vec::with_capacity(models.len());
    for model in models {
        outputs.push(to_video_output(db, model).await?);
    }
    Ok(outputs)
}
