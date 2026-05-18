use sea_orm::DatabaseConnection;
use serde_json::Value;
use crate::AppState;
use crate::error::AppError;
use crate::queue::cancellation::JobCancel;

/// Process a single media file scrape job.
pub async fn handle(
    _db: &DatabaseConnection,
    _state: &AppState,
    _job_id: &str,
    _payload: &Value,
    _cancel: &JobCancel,
) -> Result<(), AppError> {
    Ok(())
}
