//! Repo for the `playback_sessions` table — active / recently-stopped play sessions.

use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::*;
use uuid::Uuid;

use crate::{db::entities::playback_sessions, error::AppError};

// ── Input ─────────────────────────────────────────────────────────────────────

pub struct CreatePlaybackSessionInput {
    pub user_id: Uuid,
    pub session_id: Option<Uuid>,
    pub file_id: Uuid,

    pub client_name: Option<String>,
    pub user_agent: Option<String>,

    pub play_method: String,

    // Source file snapshot
    pub source_container: Option<String>,
    pub source_video_codec: Option<String>,
    pub source_video_profile: Option<String>,
    pub source_hdr_type: Option<String>,
    pub source_width: Option<i32>,
    pub source_height: Option<i32>,
    pub source_duration: Option<i32>,
    pub source_file_size: Option<i64>,

    // Transcode target
    pub transcode_video_codec: Option<String>,
    pub transcode_audio_codec: Option<String>,
    pub transcode_reasons: Option<serde_json::Value>,

    // Raw detail
    pub media_streams_raw: Option<serde_json::Value>,
    pub client_capabilities: Option<serde_json::Value>,
}

// ── Repo ──────────────────────────────────────────────────────────────────────

pub struct PlaybackSessionRepo;

impl PlaybackSessionRepo {
    /// Insert a new active session. Returns the generated session UUID.
    pub async fn create<C: ConnectionTrait>(db: &C, input: CreatePlaybackSessionInput) -> Result<Uuid, AppError> {
        let id = Uuid::new_v4();
        let now = Utc::now().fixed_offset();

        let active = playback_sessions::ActiveModel {
            id: Set(id),
            user_id: Set(input.user_id),
            session_id: Set(input.session_id),
            file_id: Set(input.file_id),
            client_name: Set(input.client_name),
            user_agent: Set(input.user_agent),
            play_method: Set(input.play_method),
            source_container: Set(input.source_container),
            source_video_codec: Set(input.source_video_codec),
            source_video_profile: Set(input.source_video_profile),
            source_hdr_type: Set(input.source_hdr_type),
            source_width: Set(input.source_width),
            source_height: Set(input.source_height),
            source_duration: Set(input.source_duration),
            source_file_size: Set(input.source_file_size),
            transcode_video_codec: Set(input.transcode_video_codec),
            transcode_audio_codec: Set(input.transcode_audio_codec),
            transcode_reasons: Set(input.transcode_reasons),
            media_streams_raw: Set(input.media_streams_raw),
            client_capabilities: Set(input.client_capabilities),
            position: Set(0),
            started_at: Set(now),
            last_seen_at: Set(now),
            stopped_at: Set(None),
        };

        playback_sessions::Entity::insert(active).exec(db).await?;
        Ok(id)
    }

    /// Update the last-known playback position and refresh the heartbeat timestamp.
    pub async fn update_progress<C: ConnectionTrait>(db: &C, session_id: Uuid, position: i32) -> Result<(), AppError> {
        playback_sessions::Entity::update_many()
            .col_expr(playback_sessions::Column::Position, Expr::value(position))
            .col_expr(playback_sessions::Column::LastSeenAt, Expr::cust("NOW()"))
            .filter(playback_sessions::Column::Id.eq(session_id))
            .filter(playback_sessions::Column::StoppedAt.is_null())
            .exec(db)
            .await?;
        Ok(())
    }

    /// Mark a session as stopped by its UUID.
    pub async fn stop<C: ConnectionTrait>(db: &C, session_id: Uuid, position: i32) -> Result<(), AppError> {
        playback_sessions::Entity::update_many()
            .col_expr(playback_sessions::Column::Position, Expr::value(position))
            .col_expr(playback_sessions::Column::StoppedAt, Expr::cust("NOW()"))
            .col_expr(playback_sessions::Column::LastSeenAt, Expr::cust("NOW()"))
            .filter(playback_sessions::Column::Id.eq(session_id))
            .filter(playback_sessions::Column::StoppedAt.is_null())
            .exec(db)
            .await?;
        Ok(())
    }

    /// Mark all active sessions for a given file as stopped (used by native stop-session endpoint).
    pub async fn stop_by_file<C: ConnectionTrait>(db: &C, file_id: Uuid) -> Result<(), AppError> {
        playback_sessions::Entity::update_many()
            .col_expr(playback_sessions::Column::StoppedAt, Expr::cust("NOW()"))
            .col_expr(playback_sessions::Column::LastSeenAt, Expr::cust("NOW()"))
            .filter(playback_sessions::Column::FileId.eq(file_id))
            .filter(playback_sessions::Column::StoppedAt.is_null())
            .exec(db)
            .await?;
        Ok(())
    }

    /// Update position + heartbeat matched by (file_id, user_id). Used by Jellyfin path.
    pub async fn update_progress_by_file<C: ConnectionTrait>(
        db: &C,
        file_id: Uuid,
        user_id: Uuid,
        position: i32,
    ) -> Result<(), AppError> {
        playback_sessions::Entity::update_many()
            .col_expr(playback_sessions::Column::Position, Expr::value(position))
            .col_expr(playback_sessions::Column::LastSeenAt, Expr::cust("NOW()"))
            .filter(playback_sessions::Column::FileId.eq(file_id))
            .filter(playback_sessions::Column::UserId.eq(user_id))
            .filter(playback_sessions::Column::StoppedAt.is_null())
            .exec(db)
            .await?;
        Ok(())
    }

    /// Mark a session stopped matched by (file_id, user_id). Used by Jellyfin path.
    pub async fn stop_by_file_and_user<C: ConnectionTrait>(
        db: &C,
        file_id: Uuid,
        user_id: Uuid,
        position: i32,
    ) -> Result<(), AppError> {
        playback_sessions::Entity::update_many()
            .col_expr(playback_sessions::Column::Position, Expr::value(position))
            .col_expr(playback_sessions::Column::StoppedAt, Expr::cust("NOW()"))
            .col_expr(playback_sessions::Column::LastSeenAt, Expr::cust("NOW()"))
            .filter(playback_sessions::Column::FileId.eq(file_id))
            .filter(playback_sessions::Column::UserId.eq(user_id))
            .filter(playback_sessions::Column::StoppedAt.is_null())
            .exec(db)
            .await?;
        Ok(())
    }
}
