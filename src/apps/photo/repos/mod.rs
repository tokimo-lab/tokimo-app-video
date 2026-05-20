// =============================================================================
// ⚠️ CROSS-APP DEPRECATED: photo repo inside video sidecar ⚠️
// =============================================================================
// COMMENTED OUT IN B4: photo entities removed from video app
// =============================================================================

/*
use sea_orm::{DatabaseConnection, EntityTrait};
use uuid::Uuid;
use serde_json::Value;
use crate::db::entities::photo_libraries;
use crate::error::AppError;

pub struct PhotoLibraryRepo;

impl PhotoLibraryRepo {
    pub async fn get_by_id(db: &DatabaseConnection, id: Uuid) -> Result<Option<photo_libraries::Model>, AppError> {
        let model = photo_libraries::Entity::find_by_id(id).one(db).await?;
        Ok(model)
    }

    pub async fn update_sync_status(
        _db: &DatabaseConnection,
        _id: Uuid,
        _status: &str,
        _completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(), AppError> {
        Ok(())
    }

    pub fn parse_sources(sources_json: &Value) -> Vec<(Uuid, String, bool)> {
        let arr = match sources_json.as_array() {
            Some(a) => a,
            None => return vec![],
        };
        arr.iter()
            .filter_map(|s| {
                let id = s.get("sourceId")?.as_str()?.parse::<Uuid>().ok()?;
                let path = s.get("rootPath")?.as_str()?.to_string();
                let is_default = s.get("isDefault").and_then(|v| v.as_bool()).unwrap_or(false);
                Some((id, path, is_default))
            })
            .collect()
    }
}
*/
