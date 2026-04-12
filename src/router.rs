use axum::{
    Router,
    routing::{delete, get, post},
};
use std::sync::Arc;

use crate::AppState;
use super::handlers;
use super::handlers::get_play_url;

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
            "/api/apps/video/item/{id}",
            get(handlers::get_video_item_detail),
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
            "/api/apps/video/{id}/items",
            get(handlers::list_video_items),
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
            "/api/apps/video/{id}/countries",
            get(handlers::list_video_countries),
        )
        .route(
            "/api/apps/video/{id}/recently-added",
            get(handlers::video_recently_added),
        )
        // ── File streaming & play-url ──────────────────────────────────────
        .route(
            "/api/video/files/{file_id}/stream",
            get(handlers::file_stream::stream_media_file),
        )
        .route("/api/video/files/{id}/play-url", get(get_play_url))
        // ── Subtitles ──────────────────────────────────────────────────────
        .route(
            "/api/apps/subtitles/file/{file_id}",
            get(handlers::subtitle::get_file_subtitles),
        )
        .route(
            "/api/apps/subtitles/search",
            post(handlers::subtitle::search),
        )
        .route(
            "/api/apps/subtitles/download",
            post(handlers::subtitle::download),
        )
        .route(
            "/api/apps/subtitles/{subtitle_id}",
            delete(handlers::subtitle::delete_subtitle),
        )
        .route(
            "/api/apps/subtitles/{subtitle_id}/events",
            get(handlers::subtitle_events::get_subtitle_events),
        )
        .route(
            "/api/apps/subtitles/{subtitle_id}/sse",
            get(handlers::subtitle_events::subtitle_events_sse),
        )
        // ── HLS transcoding sessions ───────────────────────────────────────
        .route("/api/hls/sessions", post(handlers::hls::create_session))
        .route(
            "/api/hls/{session_id}",
            delete(handlers::hls::stop_session),
        )
        .route(
            "/api/hls/by-file/{file_id}",
            delete(handlers::hls::stop_sessions_for_file),
        )
        .route(
            "/api/hls/{session_id}/playlist.m3u8",
            get(handlers::hls::get_playlist),
        )
        .route(
            "/api/hls/{session_id}/{segment}",
            get(handlers::hls::get_segment),
        )
        // ── Playback (stream-url, watch history, progress) ────────────────
        .route(
            "/api/playback/stream-url/{file_id}",
            post(handlers::playback::stream_url),
        )
        .route(
            "/api/playback/stop-session/{file_id}",
            delete(handlers::playback::stop_session_delete)
                .post(handlers::playback::stop_session_beacon),
        )
        .route(
            "/api/playback/resume-position",
            get(handlers::playback::resume_position),
        )
        .route(
            "/api/playback/watch-history",
            get(handlers::playback::watch_history),
        )
        .route(
            "/api/playback/watch-history/{id}",
            delete(handlers::playback::delete_watch_history),
        )
        .route(
            "/api/playback/progress",
            post(handlers::playback::report_progress),
        )
        .route(
            "/api/playback/state",
            get(handlers::playback_state::get_playback_state)
                .post(handlers::playback_state::save_playback_state),
        )
}
