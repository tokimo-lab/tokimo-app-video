use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaPlaybackState {
    pub state_data: Option<serde_json::Value>,
}

pub struct MediaPlaybackStateRepo;

impl MediaPlaybackStateRepo {
    pub async fn get(
        _db: &DatabaseConnection,
        _user_id: &str,
    ) -> Result<Option<MediaPlaybackState>, AppError> {
        Ok(None)
    }

    pub async fn upsert(
        _db: &DatabaseConnection,
        _user_id: &str,
        _state_data: Option<serde_json::Value>,
    ) -> Result<(), AppError> {
        Ok(())
    }
}
