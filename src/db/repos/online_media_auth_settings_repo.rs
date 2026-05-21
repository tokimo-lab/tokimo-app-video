use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use serde_json::Value;

use crate::db::entities::online_media_auth_settings;
use crate::error::AppError;

pub struct OnlineMediaAuthSettingsRepo;

impl OnlineMediaAuthSettingsRepo {
    pub async fn get_all<C: ConnectionTrait>(db: &C) -> Result<Vec<online_media_auth_settings::Model>, AppError> {
        Ok(online_media_auth_settings::Entity::find()
            .order_by_asc(online_media_auth_settings::Column::Provider)
            .all(db)
            .await?)
    }

    pub async fn get_one<C: ConnectionTrait>(
        db: &C,
        provider: &str,
    ) -> Result<Option<online_media_auth_settings::Model>, AppError> {
        Ok(online_media_auth_settings::Entity::find_by_id(provider.to_string())
            .one(db)
            .await?)
    }

    pub async fn upsert<C: ConnectionTrait>(
        db: &C,
        provider: &str,
        value: Value,
    ) -> Result<online_media_auth_settings::Model, AppError> {
        let now = Utc::now().fixed_offset();
        let active = online_media_auth_settings::ActiveModel {
            provider: Set(provider.to_string()),
            value: Set(value),
            updated_at: Set(now),
        };

        Ok(online_media_auth_settings::Entity::insert(active)
            .on_conflict(
                OnConflict::columns([online_media_auth_settings::Column::Provider])
                    .update_columns([
                        online_media_auth_settings::Column::Value,
                        online_media_auth_settings::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec_with_returning(db)
            .await?)
    }
}
