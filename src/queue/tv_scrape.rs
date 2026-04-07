use sea_orm::DatabaseConnection;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;

use crate::queue::handlers::file_scrape;

/// Process all new/changed files for a single TV show in one sequential job.
///
/// Payload schema:
/// ```json
/// {
///   "showDir": "/path/to/ShowName",
///   "appId": "uuid", "sourceId": "uuid", "libType": "tv",
///   "files": [{ "filePath": "...", "dirPath": "...", "fileSize": 123, "checksum": "123:456" }]
/// }
/// ```
///
/// Each file is scraped by delegating to `file_scrape::handle`, which performs its own
/// idempotency check and lazy TMDB loading — guaranteeing at most one TMDB API call per show
/// regardless of how many episodes are in the payload.
pub async fn handle(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    job_id: Uuid,
    payload: &JsonValue,
) -> Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>> {
    let show_dir = payload
        .get("showDir")
        .and_then(|v| v.as_str())
        .ok_or("Missing showDir")?;
    let app_id = payload.get("appId").and_then(|v| v.as_str()).ok_or("Missing appId")?;
    let source_id =
        payload.get("sourceId").and_then(|v| v.as_str()).ok_or("Missing sourceId")?;
    let lib_type = payload.get("libType").and_then(|v| v.as_str()).ok_or("Missing libType")?;
    let files = payload
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or("Missing files array")?;

    let total = files.len();
    info!("[tv_scrape] show=\"{show_dir}\" files={total}");

    let mut processed = 0u32;
    let mut errors = 0u32;

    for file in files {
        let file_path = file.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
        let file_payload = json!({
            "filePath": file.get("filePath"),
            "dirPath": file.get("dirPath"),
            "fileSize": file.get("fileSize"),
            "checksum": file.get("checksum"),
            "appId": app_id,
            "sourceId": source_id,
            "libType": lib_type,
        });

        match file_scrape::handle(db, state, job_id, &file_payload).await {
            Ok(_) => processed += 1,
            Err(e) => {
                error!("[tv_scrape] Error on \"{file_path}\": {e}");
                errors += 1;
            }
        }
    }

    info!("[tv_scrape] show=\"{show_dir}\" done: {processed}/{total} ok, {errors} errors");

    Ok(Some(json!({
        "showDir": show_dir,
        "total": total,
        "processed": processed,
        "errors": errors,
    })))
}
