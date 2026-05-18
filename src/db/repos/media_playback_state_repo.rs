use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::*;
use uuid::Uuid;

use crate::db::entities::media_playback_states;
use crate::error::AppError;

pub struct MediaPlaybackStateRepo;

impl MediaPlaybackStateRepo {
    pub async fn get(db: &DatabaseConnection, user_id: Uuid) -> Result<Option<serde_json::Value>, AppError> {
        let row = media_playback_states::Entity::find()
            .filter(media_playback_states::Column::UserId.eq(user_id))
            .one(db)
            .await?;
        Ok(row.map(|r| r.state_data))
    }

    /// Upsert playback state using JSON merge (`||`).
    /// Each caller saves only its own top-level key (e.g. `{ "music": {...} }`
    /// or `{ "appleMusic": {...} }`), and the merge preserves sibling keys.
    pub async fn upsert(db: &DatabaseConnection, user_id: Uuid, state_data: serde_json::Value) -> Result<(), AppError> {
        use sea_orm::prelude::Expr;
        use sea_orm::sea_query::OnConflict;

        let now: DateTimeWithTimeZone = chrono::Utc::now().into();
        let model = media_playback_states::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user_id),
            state_data: Set(state_data),
            updated_at: Set(now),
        };

        media_playback_states::Entity::insert(model)
            .on_conflict(
                OnConflict::column(media_playback_states::Column::UserId)
                    .value(
                        media_playback_states::Column::StateData,
                        Expr::cust("media_playback_states.state_data || EXCLUDED.state_data"),
                    )
                    .update_column(media_playback_states::Column::UpdatedAt)
                    .to_owned(),
            )
            .exec(db)
            .await?;

        Ok(())
    }
}
