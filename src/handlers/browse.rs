use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use std::sync::Arc;

use crate::db::pagination::Page;
use crate::db::repos::media::MediaContentRepo;
use crate::db::repos::media::media_content_repo::ListMediaInput;
use crate::error::{AppError, OptionExt};
use crate::handlers::{ok, ApiResponse};
use crate::AppState;

use super::{
    parse_uuid, VideoCollectionsQuery, VideoListMediaQuery, VideoRecentlyAddedQuery,
    VideoToggleFavoriteInput,
};

/// GET /api/apps/video/{id}/items
pub async fn list_video_items(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<VideoListMediaQuery>,
) -> Result<Json<ApiResponse<Page<serde_json::Value>>>, AppError> {
    let uid = parse_uuid(&id)?;
    let page = q.page.unwrap_or(1);
    let page_size = q.page_size.unwrap_or(20);
    let genre_id = q.genre_id.as_deref().map(parse_uuid).transpose()?;
    let (items, total) = MediaContentRepo::list_video_items(
        &state.db,
        ListMediaInput {
            video_id: uid,
            page,
            page_size,
            sort_by: q.sort_by.clone().unwrap_or_else(|| "title".to_string()),
            sort_dir: q.sort_dir.clone().unwrap_or_else(|| "asc".to_string()),
            genre_id,
            search: q.search.clone(),
        },
    )
    .await?;
    Ok(ok(
        Page::from_parts(items, total, page as u64, page_size as u64),
    ))
}

/// GET /api/apps/video/{id}/tv-shows
pub async fn list_video_tv_shows(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<VideoListMediaQuery>,
) -> Result<Json<ApiResponse<Page<serde_json::Value>>>, AppError> {
    let uid = parse_uuid(&id)?;
    let page = q.page.unwrap_or(1);
    let page_size = q.page_size.unwrap_or(20);
    let genre_id = q.genre_id.as_deref().map(parse_uuid).transpose()?;
    let (items, total) = MediaContentRepo::list_tv_shows(
        &state.db,
        ListMediaInput {
            video_id: uid,
            page,
            page_size,
            sort_by: q.sort_by.clone().unwrap_or_else(|| "title".to_string()),
            sort_dir: q.sort_dir.clone().unwrap_or_else(|| "asc".to_string()),
            genre_id,
            search: q.search.clone(),
        },
    )
    .await?;
    Ok(ok(
        Page::from_parts(items, total, page as u64, page_size as u64),
    ))
}

/// GET /api/apps/video/{id}/genres
pub async fn list_video_genres(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, AppError> {
    let uid = parse_uuid(&id)?;
    let items = MediaContentRepo::list_genres(&state.db, uid).await?;
    Ok(ok(items))
}

/// GET /api/apps/video/{id}/recently-added
pub async fn video_recently_added(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<VideoRecentlyAddedQuery>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, AppError> {
    let uid = parse_uuid(&id)?;
    let limit = q.limit.unwrap_or(20);
    let items = MediaContentRepo::get_recently_added(&state.db, uid, limit).await?;
    Ok(ok(items))
}

/// POST /api/apps/video/toggle-favorite
pub async fn video_toggle_favorite(
    State(state): State<Arc<AppState>>,
    Json(body): Json<VideoToggleFavoriteInput>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let uid = parse_uuid(&body.id)?;
    let is_fav = MediaContentRepo::toggle_favorite(&state.db, &body.r#type, uid).await?;
    Ok(ok(serde_json::json!({ "isFavorite": is_fav })))
}

/// GET /api/apps/video/item/{id}
pub async fn get_video_item_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let uid = parse_uuid(&id)?;
    let detail = MediaContentRepo::get_video_item_detail(&state.db, uid)
        .await?
        .not_found(format!("video item {id} not found"))?;
    Ok(ok(detail))
}

/// GET /api/apps/video/tv/{id}
pub async fn get_video_tv_show_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let uid = parse_uuid(&id)?;
    let detail = MediaContentRepo::get_tv_show_detail(&state.db, uid)
        .await?
        .not_found(format!("tv show {id} not found"))?;
    Ok(ok(detail))
}

/// GET /api/apps/video/collections
pub async fn list_video_collections(
    State(state): State<Arc<AppState>>,
    Query(q): Query<VideoCollectionsQuery>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, AppError> {
    let movie_id = q.video_item_id.as_deref().map(parse_uuid).transpose()?;
    let tv_show_id = q.tv_show_id.as_deref().map(parse_uuid).transpose()?;
    let items = MediaContentRepo::list_collections(&state.db, movie_id, tv_show_id).await?;
    Ok(ok(items))
}

/// GET /api/apps/video/person/{id}
pub async fn get_video_person_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let uid = parse_uuid(&id)?;
    let detail = MediaContentRepo::get_person_detail(&state.db, uid, "movie")
        .await?
        .not_found(format!("person {id} not found"))?;
    Ok(ok(detail))
}
