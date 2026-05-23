use axum::{
    Router,
    routing::{delete, get, post},
};
use std::sync::Arc;

use super::handlers;
use crate::AppState;

pub fn build_media_organize_app_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/media-organize/session", get(handlers::get_session))
        .route("/media-organize/scan", post(handlers::scan))
        .route("/media-organize/identify/{itemId}", post(handlers::identify_item))
        .route("/media-organize/identify-all", post(handlers::identify_all))
        .route("/media-organize/select-match", post(handlers::select_match))
        .route("/media-organize/manual-search", post(handlers::manual_search))
        .route(
            "/media-organize/manual-search-adult",
            post(handlers::manual_search_adult),
        )
        .route("/media-organize/select-adult-match", post(handlers::select_adult_match))
        .route("/media-organize/select-music-match", post(handlers::select_music_match))
        .route(
            "/media-organize/manual-search-music",
            post(handlers::manual_search_music),
        )
        .route("/media-organize/reset-match", post(handlers::reset_match))
        .route("/media-organize/update-target", post(handlers::update_target))
        .route("/media-organize/execute", post(handlers::execute))
        .route("/media-organize/cancel", post(handlers::cancel))
        .route("/media-organize/clear", post(handlers::clear))
        .route("/media-organize/reports", get(handlers::list_reports))
        .route("/media-organize/reports/{id}", get(handlers::get_report))
        .route("/media-organize/reports/{id}", delete(handlers::delete_report))
}
