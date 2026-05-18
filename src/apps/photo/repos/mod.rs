use sea_orm::DatabaseConnection;
use uuid::Uuid;
use serde_json::Value;
use crate::error::AppError;

pub struct PhotoLibrary {
    pub id: Uuid,
    pub sources: Value,
}

pub struct PhotoLibraryRepo;

impl PhotoLibraryRepo {
    pub async fn get_by_id(_db: &DatabaseConnection, _id: Uuid) -> Result<Option<PhotoLibrary>, AppError> {
        Ok(None)
    }

    pub async fn update_sync_status(
        _db: &DatabaseConnection,
        _id: Uuid,
        _status: &str,
        _completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(), AppError> {
        Ok(())
    }

    pub fn parse_sources(_sources: &Value) -> Vec<(Uuid, String)> {
        vec![]
    }
}
