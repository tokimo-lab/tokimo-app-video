use axum::{
    Router,
    routing::{delete, get, post, put},
};
use std::sync::Arc;

use super::handlers;
use super::handlers::get_play_url;
use crate::AppState;

pub fn build_video_app_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ytdlp/status", get(handlers::ytdlp::status))
        .route("/ytdlp/update", post(handlers::ytdlp::update))
        // Online media providers, ingest tasks & auth settings
        .route(
            "/online-media/health",
            get(handlers::online_media::health).post(handlers::online_media::health),
        )
        .route(
            "/online-media/providers",
            get(handlers::online_media::list_providers),
        )
        .route(
            "/online-media/auth-settings",
            get(handlers::online_media::get_auth_settings),
        )
        .route(
            "/online-media/auth-settings/{provider}",
            put(handlers::online_media::update_auth_setting),
        )
        .route("/online-media/analyze", post(handlers::online_media::analyze))
        .route("/online-media/tasks", post(handlers::online_media::create_task))
        .route(
            "/online-media/tasks/{task_id}",
            get(handlers::online_media::get_task),
        )
        .route(
            "/online-media/tasks/{task_id}/cancel",
            post(handlers::online_media::cancel_task),
        )
        .route(
            "/online-media/resolve-collection",
            post(handlers::online_media::resolve_collection),
        )
        .route(
            "/online-media/batch-tasks",
            post(handlers::online_media::batch_create_tasks),
        )
        .route(
            "/online-media/start-download",
            post(handlers::online_media::start_online_media_download),
        )
        .route(
            "/online-media/retry-download",
            post(handlers::online_media::retry_online_media_download),
        )
        .route("/image-proxy", get(handlers::image_proxy::image_proxy))
        // Category CRUD
        .route(
            "/",
            get(handlers::list_videos).post(handlers::create_video),
        )
        .route("/reorder", post(handlers::reorder_videos))
        .route(
            "/sync-statuses",
            get(handlers::get_all_video_sync_statuses),
        )
        // Global scraping settings
        .route(
            "/scraping-settings",
            get(handlers::get_video_scraping_settings)
                .put(handlers::update_video_scraping_settings),
        )
        // Cross-category content
        .route(
            "/toggle-favorite",
            post(handlers::video_toggle_favorite),
        )
        .route(
            "/collections",
            get(handlers::list_video_collections),
        )
        // Content detail (no category scope)
        .route(
            "/item/{id}",
            get(handlers::get_video_item_detail),
        )
        .route(
            "/tv/{id}",
            get(handlers::get_video_tv_show_detail),
        )
        .route(
            "/person/{id}",
            get(handlers::get_video_person_detail),
        )
        // Category-scoped routes (parameterized /{id} — must come after named routes)
        .route(
            "/{id}",
            get(handlers::get_video)
                .patch(handlers::update_video)
                .delete(handlers::delete_video),
        )
        .route(
            "/{id}/sync",
            post(handlers::sync_video),
        )
        .route(
            "/{id}/sync-status",
            get(handlers::get_video_sync_status),
        )
        .route(
            "/{id}/sync-progress",
            get(handlers::get_video_sync_progress),
        )
        .route(
            "/{id}/items",
            get(handlers::list_video_items),
        )
        .route(
            "/{id}/tv-shows",
            get(handlers::list_video_tv_shows),
        )
        .route(
            "/{id}/genres",
            get(handlers::list_video_genres),
        )
        .route(
            "/{id}/countries",
            get(handlers::list_video_countries),
        )
        .route(
            "/{id}/recently-added",
            get(handlers::video_recently_added),
        )
        // ── File streaming & play-url ──────────────────────────────────────
        .route(
            "/files/{file_id}/stream",
            get(handlers::file_stream::stream_media_file),
        )
        .route("/files/{id}/play-url", get(get_play_url))
        // ── Subtitles ──────────────────────────────────────────────────────
        .route(
            "/subtitles/file/{file_id}",
            get(handlers::subtitle::get_file_subtitles),
        )
        .route(
            "/subtitles/search",
            post(handlers::subtitle::search),
        )
        .route(
            "/subtitles/download",
            post(handlers::subtitle::download),
        )
        .route(
            "/subtitles/{subtitle_id}",
            delete(handlers::subtitle::delete_subtitle),
        )
        .route(
            "/subtitles/{subtitle_id}/events",
            get(handlers::subtitle_events::get_subtitle_events),
        )
        .route(
            "/subtitles/{subtitle_id}/sse",
            get(handlers::subtitle_events::subtitle_events_sse),
        )
        // ── HLS transcoding sessions ───────────────────────────────────────
        .route("/hls/sessions", post(handlers::hls::create_session))
        .route(
            "/hls/{session_id}",
            delete(handlers::hls::stop_session),
        )
        .route(
            "/hls/by-file/{file_id}",
            delete(handlers::hls::stop_sessions_for_file),
        )
        .route(
            "/hls/{session_id}/playlist.m3u8",
            get(handlers::hls::get_playlist),
        )
        .route(
            "/hls/{session_id}/{segment}",
            get(handlers::hls::get_segment),
        )
        // ── Playback (stream-url, watch history, progress) ────────────────
        .route(
            "/playback/stream-url/{file_id}",
            post(handlers::playback::stream_url),
        )
        .route(
            "/playback/stop-session/{file_id}",
            delete(handlers::playback::stop_session_delete)
                .post(handlers::playback::stop_session_beacon),
        )
        .route(
            "/playback/resume-position",
            get(handlers::playback::resume_position),
        )
        .route(
            "/playback/watch-history",
            get(handlers::playback::watch_history),
        )
        .route(
            "/playback/watch-history/{id}",
            delete(handlers::playback::delete_watch_history),
        )
        .route(
            "/playback/progress",
            post(handlers::playback::report_progress),
        )
}
