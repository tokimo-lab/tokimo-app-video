use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;

use crate::AppState;

use super::handlers;

pub fn build_video_app_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Category CRUD
        .route(
            "/api/apps/video",
            get(handlers::list_videos).post(handlers::create_video),
        )
        .route("/api/apps/video/reorder", post(handlers::reorder_videos))
        .route(
            "/api/apps/video/sync-statuses",
            get(handlers::get_all_video_sync_statuses),
        )
        // Global scraping settings
        .route(
            "/api/apps/video/scraping-settings",
            get(handlers::get_video_scraping_settings)
                .put(handlers::update_video_scraping_settings),
        )
        // Cross-category content
        .route(
            "/api/apps/video/toggle-favorite",
            post(handlers::video_toggle_favorite),
        )
        .route(
            "/api/apps/video/collections",
            get(handlers::list_video_collections),
        )
        // Content detail (no category scope)
        .route(
            "/api/apps/video/movie/{id}",
            get(handlers::get_video_movie_detail),
        )
        .route(
            "/api/apps/video/tv/{id}",
            get(handlers::get_video_tv_show_detail),
        )
        .route(
            "/api/apps/video/person/{id}",
            get(handlers::get_video_person_detail),
        )
        // Category-scoped routes (parameterized /{id} — must come after named routes)
        .route(
            "/api/apps/video/{id}",
            get(handlers::get_video)
                .patch(handlers::update_video)
                .delete(handlers::delete_video),
        )
        .route(
            "/api/apps/video/{id}/sync",
            post(handlers::sync_video),
        )
        .route(
            "/api/apps/video/{id}/sync-status",
            get(handlers::get_video_sync_status),
        )
        .route(
            "/api/apps/video/{id}/sync-progress",
            get(handlers::get_video_sync_progress),
        )
        .route(
            "/api/apps/video/{id}/movies",
            get(handlers::list_video_movies),
        )
        .route(
            "/api/apps/video/{id}/tv-shows",
            get(handlers::list_video_tv_shows),
        )
        .route(
            "/api/apps/video/{id}/genres",
            get(handlers::list_video_genres),
        )
        .route(
            "/api/apps/video/{id}/recently-added",
            get(handlers::video_recently_added),
        )
}
