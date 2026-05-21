use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use serde_json::Value;

use crate::db::entities::ytdlp_provider_auth;
use crate::error::AppError;

pub struct YtdlpProviderAuthRepo;

impl YtdlpProviderAuthRepo {
    pub async fn get_all<C: ConnectionTrait>(db: &C) -> Result<Vec<ytdlp_provider_auth::Model>, AppError> {
        Ok(ytdlp_provider_auth::Entity::find()
            .order_by_asc(ytdlp_provider_auth::Column::Provider)
            .all(db)
            .await?)
    }

    pub async fn get_one<C: ConnectionTrait>(
        db: &C,
        provider: &str,
    ) -> Result<Option<ytdlp_provider_auth::Model>, AppError> {
        Ok(ytdlp_provider_auth::Entity::find_by_id(provider.to_string())
            .one(db)
            .await?)
    }

    pub async fn upsert<C: ConnectionTrait>(
        db: &C,
        provider: &str,
        value: Value,
    ) -> Result<ytdlp_provider_auth::Model, AppError> {
        let now = Utc::now().fixed_offset();
        let active = ytdlp_provider_auth::ActiveModel {
            provider: Set(provider.to_string()),
            value: Set(value),
            updated_at: Set(now),
        };

        Ok(ytdlp_provider_auth::Entity::insert(active)
            .on_conflict(
                OnConflict::columns([ytdlp_provider_auth::Column::Provider])
                    .update_columns([
                        ytdlp_provider_auth::Column::Value,
                        ytdlp_provider_auth::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec_with_returning(db)
            .await?)
    }
}
