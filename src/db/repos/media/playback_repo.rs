use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, ExprTrait, QueryFilter, QueryOrder,
    TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use uuid::Uuid;

use crate::db::datetime::ApiDateTimeExt;

use crate::db::entities::{user_media_states, video_files, watch_histories};
use crate::db::models::playback::{ResumePositionDto, WatchHistoryItemDto};
use crate::error::AppError;

/// Input for inserting a watch history record.
#[derive(Debug)]
pub struct InsertHistoryInput {
    pub id: Uuid,
    pub user_id: Uuid,
    pub file_id: Uuid,
    pub client_name: String,
    pub user_agent: Option<String>,
    pub position: i32,
    pub duration: Option<i32>,
    pub user_display_name_snapshot: Option<String>,
}

pub struct PlaybackRepo;

impl PlaybackRepo {
    // ── query endpoints ────────────────────────────────────────────────────

    pub async fn get_resume_position<C: ConnectionTrait>(
        db: &C,
        user_id: Uuid,
        movie_id: Option<Uuid>,
        episode_id: Option<Uuid>,
    ) -> Result<ResumePositionDto, AppError> {
        if let Some(mid) = movie_id {
            let state = user_media_states::Entity::find()
                .filter(user_media_states::Column::UserId.eq(user_id))
                .filter(user_media_states::Column::VideoItemId.eq(mid))
                .one(db)
                .await?;

            let Some(s) = state else {
                return Ok(ResumePositionDto::default());
            };

            let file = video_files::Entity::find()
                .filter(video_files::Column::VideoItemId.eq(mid))
                .one(db)
                .await?;

            return Ok(ResumePositionDto {
                position: s.resume_position,
                duration: file.and_then(|f| f.duration),
                is_watched: s.is_watched,
                play_count: s.play_count,
                last_watch_at: s.last_watch_at.to_api_datetime(),
            });
        }

        if let Some(eid) = episode_id {
            let state = user_media_states::Entity::find()
                .filter(user_media_states::Column::UserId.eq(user_id))
                .filter(user_media_states::Column::EpisodeId.eq(eid))
                .one(db)
                .await?;

            let Some(s) = state else {
                return Ok(ResumePositionDto::default());
            };

            let file = video_files::Entity::find()
                .filter(video_files::Column::EpisodeId.eq(eid))
                .one(db)
                .await?;

            return Ok(ResumePositionDto {
                position: s.resume_position,
                duration: file.and_then(|f| f.duration),
                is_watched: s.is_watched,
                play_count: s.play_count,
                last_watch_at: s.last_watch_at.to_api_datetime(),
            });
        }

        Ok(ResumePositionDto::default())
    }

    pub async fn get_watch_history(
        db: &DatabaseConnection,
        user_id: Uuid,
        movie_id: Option<Uuid>,
        episode_id: Option<Uuid>,
        tv_show_id: Option<Uuid>,
        limit: u64,
    ) -> Result<Vec<WatchHistoryItemDto>, AppError> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

        // Whether to include episode info (season/episode number) in the result
        let include_episode_info = tv_show_id.is_some();

        let (filter_sql, filter_val): (String, Option<Uuid>) = if let Some(mid) = movie_id {
            (
                "JOIN video_files vf ON wh.file_id = vf.id WHERE wh.user_id = $1 AND vf.video_item_id = $2".to_string(),
                Some(mid),
            )
        } else if let Some(eid) = episode_id {
            (
                "JOIN video_files vf ON wh.file_id = vf.id WHERE wh.user_id = $1 AND vf.episode_id = $2".to_string(),
                Some(eid),
            )
        } else if let Some(tid) = tv_show_id {
            (
                "JOIN video_files vf ON wh.file_id = vf.id \
                 JOIN episodes ep ON vf.episode_id = ep.id \
                 JOIN seasons sn ON ep.season_id = sn.id \
                 WHERE wh.user_id = $1 AND ep.tv_show_id = $2"
                    .to_string(),
                Some(tid),
            )
        } else {
            ("WHERE wh.user_id = $1".to_string(), None)
        };

        let episode_cols = if include_episode_info {
            ", vf.episode_id, ep.episode_number, sn.season_number"
        } else {
            ""
        };

        let sql = if filter_val.is_some() {
            format!(
                "SELECT wh.id, wh.file_id, COALESCE(wh.user_display_name_snapshot, wh.user_id::TEXT) AS user_name, \
                        wh.client_name, wh.user_agent, wh.started_at, wh.stopped_at, \
                        wh.position, wh.duration, wh.completed{episode_cols} \
                 FROM watch_histories wh \
                 {filter_sql} \
                 ORDER BY wh.started_at DESC LIMIT $3"
            )
        } else {
            format!(
                "SELECT wh.id, wh.file_id, COALESCE(wh.user_display_name_snapshot, wh.user_id::TEXT) AS user_name, \
                        wh.client_name, wh.user_agent, wh.started_at, wh.stopped_at, \
                        wh.position, wh.duration, wh.completed{episode_cols} \
                 FROM watch_histories wh \
                 {filter_sql} \
                 ORDER BY wh.started_at DESC LIMIT $2"
            )
        };

        let stmt = if let Some(fval) = filter_val {
            Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                &sql,
                [user_id.into(), fval.into(), (limit as i64).into()],
            )
        } else {
            Statement::from_sql_and_values(DatabaseBackend::Postgres, &sql, [user_id.into(), (limit as i64).into()])
        };

        let rows = db.query_all_raw(stmt).await?;
        let items = rows
            .into_iter()
            .map(|row| WatchHistoryItemDto {
                id: row.try_get::<Uuid>("", "id").unwrap_or_default().to_string(),
                file_id: row.try_get::<Uuid>("", "file_id").ok().map(|u| u.to_string()),
                user_name: row.try_get("", "user_name").ok(),
                client_name: row.try_get("", "client_name").ok(),
                user_agent: row.try_get("", "user_agent").ok(),
                started_at: row
                    .try_get::<chrono::DateTime<chrono::FixedOffset>>("", "started_at")
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default(),
                stopped_at: row
                    .try_get::<chrono::DateTime<chrono::FixedOffset>>("", "stopped_at")
                    .ok()
                    .map(|t| t.to_rfc3339()),
                position: row.try_get("", "position").unwrap_or(0),
                duration: row.try_get("", "duration").ok(),
                completed: row.try_get("", "completed").unwrap_or(false),
                episode_id: if include_episode_info {
                    row.try_get::<Uuid>("", "episode_id").ok().map(|u| u.to_string())
                } else {
                    None
                },
                season_number: if include_episode_info {
                    row.try_get("", "season_number").ok()
                } else {
                    None
                },
                episode_number: if include_episode_info {
                    row.try_get("", "episode_number").ok()
                } else {
                    None
                },
            })
            .collect();

        Ok(items)
    }

    pub async fn delete_watch_history(
        db: &DatabaseConnection,
        user_id: Uuid,
        history_id: Uuid,
    ) -> Result<bool, AppError> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM watch_histories WHERE id = $1 AND user_id = $2",
            [history_id.into(), user_id.into()],
        );
        let result = db.execute_raw(stmt).await.map_err(AppError::from)?;
        Ok(result.rows_affected() > 0)
    }

    // ── user_media_states upserts ──────────────────────────────────────────

    pub async fn upsert_movie_state_completed<C: ConnectionTrait>(
        db: &C,
        user_id: Uuid,
        movie_id: Uuid,
        position: i32,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().into();
        let model = user_media_states::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user_id),
            video_item_id: Set(Some(movie_id)),
            tv_show_id: Set(None),
            episode_id: Set(None),
            book_id: Set(None),
            chapter_id: Set(None),
            resume_position: Set(position),
            play_count: Set(1),
            is_watched: Set(true),
            last_watch_at: Set(Some(now)),
            updated_at: Set(now),
        };
        user_media_states::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    user_media_states::Column::UserId,
                    user_media_states::Column::VideoItemId,
                ])
                .update_column(user_media_states::Column::ResumePosition)
                .value(
                    user_media_states::Column::PlayCount,
                    Expr::col((user_media_states::Entity, user_media_states::Column::PlayCount)).add(1),
                )
                .value(user_media_states::Column::IsWatched, Expr::val(true))
                .value(user_media_states::Column::LastWatchAt, Expr::cust("NOW()"))
                .value(user_media_states::Column::UpdatedAt, Expr::cust("NOW()"))
                .to_owned(),
            )
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn upsert_movie_state_in_progress<C: ConnectionTrait>(
        db: &C,
        user_id: Uuid,
        movie_id: Uuid,
        position: i32,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().into();
        let model = user_media_states::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user_id),
            video_item_id: Set(Some(movie_id)),
            tv_show_id: Set(None),
            episode_id: Set(None),
            book_id: Set(None),
            chapter_id: Set(None),
            resume_position: Set(position),
            play_count: Set(0),
            is_watched: Set(false),
            last_watch_at: Set(Some(now)),
            updated_at: Set(now),
        };
        user_media_states::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    user_media_states::Column::UserId,
                    user_media_states::Column::VideoItemId,
                ])
                .update_column(user_media_states::Column::ResumePosition)
                .value(user_media_states::Column::LastWatchAt, Expr::cust("NOW()"))
                .value(user_media_states::Column::UpdatedAt, Expr::cust("NOW()"))
                .to_owned(),
            )
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn upsert_episode_state_completed<C: ConnectionTrait>(
        db: &C,
        user_id: Uuid,
        episode_id: Uuid,
        position: i32,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().into();
        let model = user_media_states::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user_id),
            video_item_id: Set(None),
            tv_show_id: Set(None),
            episode_id: Set(Some(episode_id)),
            book_id: Set(None),
            chapter_id: Set(None),
            resume_position: Set(position),
            play_count: Set(1),
            is_watched: Set(true),
            last_watch_at: Set(Some(now)),
            updated_at: Set(now),
        };
        user_media_states::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([user_media_states::Column::UserId, user_media_states::Column::EpisodeId])
                    .update_column(user_media_states::Column::ResumePosition)
                    .value(
                        user_media_states::Column::PlayCount,
                        Expr::col((user_media_states::Entity, user_media_states::Column::PlayCount)).add(1),
                    )
                    .value(user_media_states::Column::IsWatched, Expr::val(true))
                    .value(user_media_states::Column::LastWatchAt, Expr::cust("NOW()"))
                    .value(user_media_states::Column::UpdatedAt, Expr::cust("NOW()"))
                    .to_owned(),
            )
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn upsert_episode_state_in_progress<C: ConnectionTrait>(
        db: &C,
        user_id: Uuid,
        episode_id: Uuid,
        position: i32,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().into();
        let model = user_media_states::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user_id),
            video_item_id: Set(None),
            tv_show_id: Set(None),
            episode_id: Set(Some(episode_id)),
            book_id: Set(None),
            chapter_id: Set(None),
            resume_position: Set(position),
            play_count: Set(0),
            is_watched: Set(false),
            last_watch_at: Set(Some(now)),
            updated_at: Set(now),
        };
        user_media_states::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([user_media_states::Column::UserId, user_media_states::Column::EpisodeId])
                    .update_column(user_media_states::Column::ResumePosition)
                    .value(user_media_states::Column::LastWatchAt, Expr::cust("NOW()"))
                    .value(user_media_states::Column::UpdatedAt, Expr::cust("NOW()"))
                    .to_owned(),
            )
            .exec(db)
            .await?;
        Ok(())
    }

    // ── watch_histories ────────────────────────────────────────────────────

    /// Find an active (non-stopped) watch history entry for this user+file
    /// started within the last 8 hours.
    pub async fn find_active_history<C: ConnectionTrait>(
        db: &C,
        user_id: Uuid,
        file_id: Uuid,
    ) -> Result<Option<Uuid>, AppError> {
        let eight_hours_ago = chrono::Utc::now() - chrono::Duration::hours(8);
        let row = watch_histories::Entity::find()
            .filter(watch_histories::Column::UserId.eq(user_id))
            .filter(watch_histories::Column::FileId.eq(file_id))
            .filter(watch_histories::Column::StoppedAt.is_null())
            .filter(watch_histories::Column::StartedAt.gt(eight_hours_ago))
            .order_by_desc(watch_histories::Column::StartedAt)
            .one(db)
            .await?;
        Ok(row.map(|r| r.id))
    }

    pub async fn update_history_completed<C: ConnectionTrait>(
        db: &C,
        history_id: Uuid,
        position: i32,
        duration: Option<i32>,
    ) -> Result<(), AppError> {
        watch_histories::Entity::update_many()
            .filter(watch_histories::Column::Id.eq(history_id))
            .col_expr(watch_histories::Column::Position, Expr::value(position))
            .col_expr(watch_histories::Column::Duration, Expr::value(duration))
            .col_expr(watch_histories::Column::Completed, Expr::value(true))
            .col_expr(
                watch_histories::Column::StoppedAt,
                Expr::value(Some::<sea_orm::prelude::DateTimeWithTimeZone>(
                    chrono::Utc::now().into(),
                )),
            )
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn update_history_in_progress<C: ConnectionTrait>(
        db: &C,
        history_id: Uuid,
        position: i32,
        duration: Option<i32>,
    ) -> Result<(), AppError> {
        watch_histories::Entity::update_many()
            .filter(watch_histories::Column::Id.eq(history_id))
            .col_expr(watch_histories::Column::Position, Expr::value(position))
            .col_expr(watch_histories::Column::Duration, Expr::value(duration))
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn insert_history_completed<C: ConnectionTrait>(
        db: &C,
        input: InsertHistoryInput,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().into();
        let active = watch_histories::ActiveModel {
            id: Set(input.id),
            user_id: Set(input.user_id),
            file_id: Set(input.file_id),
            session_id: Set(None),
            client_name: Set(Some(input.client_name)),
            user_agent: Set(input.user_agent),
            position: Set(input.position),
            duration: Set(input.duration),
            completed: Set(true),
            started_at: Set(now),
            stopped_at: Set(Some(now)),
            user_display_name_snapshot: Set(input.user_display_name_snapshot),
        };
        watch_histories::Entity::insert(active).exec(db).await?;
        Ok(())
    }

    pub async fn insert_history_in_progress<C: ConnectionTrait>(
        db: &C,
        input: InsertHistoryInput,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().into();
        let active = watch_histories::ActiveModel {
            id: Set(input.id),
            user_id: Set(input.user_id),
            file_id: Set(input.file_id),
            session_id: Set(None),
            client_name: Set(Some(input.client_name)),
            user_agent: Set(input.user_agent),
            position: Set(input.position),
            duration: Set(input.duration),
            completed: Set(false),
            started_at: Set(now),
            stopped_at: Set(None),
            user_display_name_snapshot: Set(input.user_display_name_snapshot),
        };
        watch_histories::Entity::insert(active).exec(db).await?;
        Ok(())
    }

    // ── progress reporting ─────────────────────────────────────────────────

    /// Update a watch history record with the latest playback progress.
    /// Also updates `user_media_states` for resume position tracking.
    /// Returns `true` if the playback is now considered completed.
    pub async fn report_progress(
        db: &DatabaseConnection,
        user_id: Uuid,
        history_id: Uuid,
        position: i32,
        duration: Option<i32>,
    ) -> Result<bool, AppError> {
        const COMPLETION_THRESHOLD_SECS: i32 = 12;

        let completed = duration.is_some_and(|d| d > 0 && position + COMPLETION_THRESHOLD_SECS >= d);

        let txn = db.begin().await?;

        // Update the watch_histories record
        let mut update = watch_histories::Entity::update_many()
            .filter(watch_histories::Column::Id.eq(history_id))
            .filter(watch_histories::Column::UserId.eq(user_id))
            .col_expr(watch_histories::Column::Position, Expr::value(position))
            .col_expr(watch_histories::Column::Duration, Expr::value(duration));

        if completed {
            update = update
                .col_expr(watch_histories::Column::Completed, Expr::value(true))
                .col_expr(
                    watch_histories::Column::StoppedAt,
                    Expr::value(Some::<sea_orm::prelude::DateTimeWithTimeZone>(
                        chrono::Utc::now().into(),
                    )),
                );
        }

        let result = update.exec(&txn).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound("watch history not found".into()));
        }

        // Look up video_item_id / episode_id from file
        let history = watch_histories::Entity::find_by_id(history_id).one(&txn).await?;
        if let Some(history) = history {
            let file = video_files::Entity::find_by_id(history.file_id).one(&txn).await?;
            if let Some(file) = file {
                if let Some(movie_id) = file.video_item_id {
                    if completed {
                        Self::upsert_movie_state_completed(&txn, user_id, movie_id, position).await?;
                    } else {
                        Self::upsert_movie_state_in_progress(&txn, user_id, movie_id, position).await?;
                    }
                } else if let Some(episode_id) = file.episode_id {
                    if completed {
                        Self::upsert_episode_state_completed(&txn, user_id, episode_id, position).await?;
                    } else {
                        Self::upsert_episode_state_in_progress(&txn, user_id, episode_id, position).await?;
                    }
                }
            }
        }

        txn.commit().await?;
        Ok(completed)
    }

    /// Create a new watch history record (for stream-url playback start).
    pub async fn create_history<C: ConnectionTrait>(
        db: &C,
        user_id: Uuid,
        file_id: Uuid,
        user_agent: Option<String>,
        duration: Option<i32>,
        user_display_name: Option<String>,
    ) -> Result<Uuid, AppError> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().into();
        let active = watch_histories::ActiveModel {
            id: Set(id),
            user_id: Set(user_id),
            file_id: Set(file_id),
            session_id: Set(None),
            client_name: Set(None),
            user_agent: Set(user_agent),
            position: Set(0),
            duration: Set(duration),
            completed: Set(false),
            started_at: Set(now),
            stopped_at: Set(None),
            user_display_name_snapshot: Set(user_display_name),
        };
        watch_histories::Entity::insert(active).exec(db).await?;
        Ok(id)
    }

    /// Verify that a watch history record belongs to the given user.
    pub async fn verify_history_ownership<C: ConnectionTrait>(
        db: &C,
        history_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, AppError> {
        let row = watch_histories::Entity::find_by_id(history_id)
            .filter(watch_histories::Column::UserId.eq(user_id))
            .one(db)
            .await?;
        Ok(row.is_some())
    }
}
