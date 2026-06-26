use axum::{
    extract::{Path, State},
    response::Json,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;
use crate::bus_clients::jobs::{self as jobs_client, video_library_filter};
use crate::db::models::video::VideoOutput;
use crate::db::repos::media::VideoRepo;
use crate::db::repos::media::video_repo::UpdateVideoFields;
use crate::error::AppError;
use crate::error::OptionExt;
use crate::handlers::user::AuthUser;
use crate::handlers::{ApiResponse, ok, ok_empty};
use crate::services::media::app_sync::AppSyncService;
use crate::services::media::source::normalize_source_path;

use super::{
    CreateVideoInput, UpdateVideoInput, VideoReorderInput, parse_uuid, sources_to_json, to_video_output,
    to_video_outputs,
};

/// GET /api/apps/video
pub async fn list_videos(State(state): State<Arc<AppState>>) -> Result<Json<ApiResponse<Vec<VideoOutput>>>, AppError> {
    let rows = VideoRepo::list_all(&state.db).await?;
    let outputs = to_video_outputs(&state.db, rows).await?;
    Ok(ok(outputs))
}

/// GET /api/apps/video/{id}
pub async fn get_video(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<VideoOutput>>, AppError> {
    let uid = parse_uuid(&id)?;
    let model = VideoRepo::get_by_id(&state.db, uid)
        .await?
        .not_found(format!("video {id} not found"))?;
    let output = to_video_output(&state.db, model).await?;
    Ok(ok(output))
}

/// POST /api/apps/video
pub async fn create_video(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateVideoInput>,
) -> Result<Json<ApiResponse<VideoOutput>>, AppError> {
    let model = VideoRepo::create(&state.db, body.name, body.r#type, body.settings).await?;
    let video_id = model.id;

    // Update optional fields
    let mut needs_update = false;
    let mut update_fields = UpdateVideoFields {
        name: None,
        r#type: None,
        description: body.description,
        avatar: body.avatar,
        poster_path: None,
        scrape_enabled: body.scrape_enabled,
        scrape_agents: body.scrape_agents,
        settings: None,
        sources: None,
    };

    if update_fields.avatar.is_some()
        || update_fields.description.is_some()
        || update_fields.scrape_enabled.is_some()
        || update_fields.scrape_agents.is_some()
    {
        needs_update = true;
    }

    // Build and set sources JSON
    if let Some(sources) = body.sources {
        // Validate source UUIDs and paths
        for s in &sources {
            let _: Uuid = s
                .source_id
                .parse()
                .map_err(|_| AppError::BadRequest("invalid source_id".into()))?;
            normalize_source_path(&s.root_path).map_err(AppError::BadRequest)?;
        }
        update_fields.sources = Some(sources_to_json(&sources));
        needs_update = true;
    }

    if needs_update {
        VideoRepo::update(&state.db, video_id, update_fields).await?;
    }

    let model = VideoRepo::get_by_id(&state.db, video_id)
        .await?
        .internal("failed to fetch created video")?;
    let output = to_video_output(&state.db, model).await?;
    Ok(ok(output))
}

/// PATCH /api/apps/video/{id}
pub async fn update_video(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateVideoInput>,
) -> Result<Json<ApiResponse<VideoOutput>>, AppError> {
    let uid = parse_uuid(&id)?;

    let _existing = VideoRepo::get_by_id(&state.db, uid)
        .await?
        .not_found(format!("video {id} not found"))?;

    let mut update_fields = UpdateVideoFields {
        name: body.name,
        r#type: body.r#type,
        description: body.description,
        avatar: body.avatar,
        poster_path: None,
        scrape_enabled: body.scrape_enabled,
        scrape_agents: body.scrape_agents,
        settings: body.settings,
        sources: None,
    };

    if let Some(ref sources) = body.sources {
        for s in sources {
            let _: Uuid = s
                .source_id
                .parse()
                .map_err(|_| AppError::BadRequest("invalid source_id".into()))?;
            normalize_source_path(&s.root_path).map_err(AppError::BadRequest)?;
        }
        update_fields.sources = Some(sources_to_json(sources));
    }

    VideoRepo::update(&state.db, uid, update_fields).await?;

    let model = VideoRepo::get_by_id(&state.db, uid)
        .await?
        .internal("failed to fetch updated video")?;
    let output = to_video_output(&state.db, model).await?;
    Ok(ok(output))
}

/// DELETE /api/apps/video/{id}
pub async fn delete_video(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let uid = parse_uuid(&id)?;
    let user_id: Uuid = auth
        .user_id
        .parse()
        .map_err(|_| AppError::Unauthorized("invalid user_id in auth token".into()))?;
    let video = VideoRepo::get_by_id(&state.db, uid)
        .await?
        .not_found(format!("video {id} not found"))?;
    let client = state.bus_client.get().expect("bus_client not initialized");
    let filter = video_library_filter(uid, None);
    let cancelled = jobs_client::cancel_by_filter(client, client.auto_caller("video"), filter).await?;
    if cancelled > 0 {
        tracing::info!("Cancelled {cancelled} jobs for deleted video category {uid}");
    }
    AppSyncService::delete_person_sources_for_library(&state.db, client, uid, &video.r#type, user_id).await?;
    VideoRepo::delete(&state.db, uid).await?;
    Ok(ok_empty())
}

/// POST /api/apps/video/reorder
pub async fn reorder_videos(
    State(state): State<Arc<AppState>>,
    Json(body): Json<VideoReorderInput>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let orders: Vec<(Uuid, i32)> = body
        .orders
        .into_iter()
        .filter_map(|item| item.id.parse::<Uuid>().ok().map(|uid| (uid, item.sort_order)))
        .collect();
    VideoRepo::reorder(&state.db, orders).await?;
    Ok(ok_empty())
}
