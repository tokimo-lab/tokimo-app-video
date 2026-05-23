//! `JellyfinAppState` implementation for the host `AppState`.

use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use sea_orm::DatabaseConnection;
use tower::util::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

use crate::{
    AppState,
    db::entities::video_files,
    db::repos::media::{
        file_repo::VideoFileRepo,
        playback_session_repo::{CreatePlaybackSessionInput, PlaybackSessionRepo},
    },
    handlers::media::{stream::stream_driver_file, utils::resolve_local_path},
};
use sea_orm::EntityTrait;
use tokimo_media_server_bridge::JellyfinPlaybackSession;

impl tokimo_media_server_bridge::JellyfinAppState for AppState {
    fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    fn server_id(&self) -> &str {
        // Stable per-instance identifier. Falls back to a fixed UUID.
        static ID: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            std::env::var("JELLYFIN_SERVER_ID").unwrap_or_else(|_| "d4e5f6a7-b8c9-0d1e-2f3a-4b5c6d7e8f90".to_string())
        });
        &ID
    }

    fn server_name(&self) -> &str {
        static NAME: std::sync::LazyLock<String> =
            std::sync::LazyLock::new(|| std::env::var("JELLYFIN_SERVER_NAME").unwrap_or_else(|_| "Tokimo".to_string()));
        &NAME
    }

    fn public_base_url(&self) -> &str {
        static URL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:5678".to_string())
        });
        &URL
    }

    async fn stream_video_file(&self, file_id: Uuid, headers: HeaderMap) -> Response {
        let file_id_str = file_id.to_string();
        let target = match VideoFileRepo::load_stream_target(&self.db, &file_id_str).await {
            Ok(Some(t)) => t,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(e) => {
                tracing::error!("jellyfin stream lookup: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

        // Local filesystem: use ServeFile for efficiency (no subtitle tap for Jellyfin)
        if target.source_type.as_deref().is_some_and(|t| t == "local") {
            let abs_path = resolve_local_path(&target.path, target.source_config.as_ref());
            let req = axum::http::Request::builder().method(axum::http::Method::GET);

            // Forward Range header
            let mut req = if let Some(range) = headers.get(header::RANGE) {
                req.header(header::RANGE, range)
            } else {
                req
            };

            // Forward If-Range
            if let Some(ir) = headers.get(header::IF_RANGE) {
                req = req.header(header::IF_RANGE, ir);
            }

            let req = req.body(Body::empty()).expect("valid request");

            match ServeFile::new(&abs_path).oneshot(req).await {
                Ok(resp) => return resp.map(Body::new).into_response(),
                Err(never) => match never {},
            }
        }

        // Remote filesystem: use VFS stream driver
        let Some(source_id) = target.source_id.as_deref() else {
            return StatusCode::NOT_FOUND.into_response();
        };

        let vfs = match self.sources.ensure_vfs(source_id).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("jellyfin vfs: {e}");
                return StatusCode::NOT_FOUND.into_response();
            }
        };

        // No subtitle tap for Jellyfin (Infuse handles subtitles itself)
        stream_driver_file(
            vfs,
            target.path,
            headers,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
    }

    async fn create_playback_session(&self, session: JellyfinPlaybackSession) {
        let Ok(Some(file)) = video_files::Entity::find_by_id(session.file_id).one(&self.db).await else {
            return;
        };

        let media_streams = serde_json::json!({
            "video": file.video_streams,
            "audio": file.audio_streams,
        });
        let source_container = std::path::Path::new(&file.path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        let input = CreatePlaybackSessionInput {
            user_id: session.user_id,
            session_id: None,
            file_id: session.file_id,
            client_name: session.client_name,
            user_agent: session.user_agent,
            play_method: "DirectPlay".to_string(),
            source_container,
            source_video_codec: file.video_codec,
            source_video_profile: file.video_profile,
            source_hdr_type: file.hdr_type,
            source_width: file.video_width,
            source_height: file.video_height,
            source_duration: file.duration,
            source_file_size: file.size,
            transcode_video_codec: None,
            transcode_audio_codec: None,
            transcode_reasons: None,
            media_streams_raw: Some(media_streams),
            client_capabilities: None,
        };

        if let Err(e) = PlaybackSessionRepo::create(&self.db, input).await {
            tracing::warn!("[Jellyfin] failed to record playback session: {e}");
        }
    }

    async fn update_playback_session_progress(&self, user_id: Uuid, file_id: Uuid, position: i32) {
        if let Err(e) = PlaybackSessionRepo::update_progress_by_file(&self.db, file_id, user_id, position).await {
            tracing::warn!("[Jellyfin] failed to update playback session progress: {e}");
        }
    }

    async fn stop_playback_session(&self, user_id: Uuid, file_id: Uuid, position: i32) {
        if let Err(e) = PlaybackSessionRepo::stop_by_file_and_user(&self.db, file_id, user_id, position).await {
            tracing::warn!("[Jellyfin] failed to stop playback session: {e}");
        }
    }
}
