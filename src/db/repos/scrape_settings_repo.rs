use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::db::entities::scrape_settings;
use crate::error::AppError;

pub const SCRAPE_SETTINGS_SINGLETON_ID: Uuid = Uuid::from_u128(0x7f5f3c2a4a8a4a5b9f2a3a6f0d7e1b42);

pub trait ScrapeSettingsSection: Serialize + DeserializeOwned + Send + Sync {
    fn default_value() -> Self;
}

pub struct ScrapeSettingsRepo;

impl ScrapeSettingsRepo {
    pub async fn get<T: ScrapeSettingsSection>(db: &impl ConnectionTrait) -> Result<T, AppError> {
        let row = scrape_settings::Entity::find_by_id(SCRAPE_SETTINGS_SINGLETON_ID)
            .one(db)
            .await?;
        match row {
            Some(m) => Ok(serde_json::from_value(m.settings_json)?),
            None => Ok(T::default_value()),
        }
    }

    pub async fn set<T: ScrapeSettingsSection>(db: &impl ConnectionTrait, value: &T) -> Result<(), AppError> {
        let settings_json = serde_json::to_value(value)?;
        let now = Utc::now().fixed_offset();
        scrape_settings::Entity::insert(scrape_settings::ActiveModel {
            id: Set(SCRAPE_SETTINGS_SINGLETON_ID),
            settings_json: Set(settings_json),
            updated_at: Set(now),
        })
        .on_conflict(
            OnConflict::column(scrape_settings::Column::Id)
                .update_columns([
                    scrape_settings::Column::SettingsJson,
                    scrape_settings::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;
        Ok(())
    }
}
