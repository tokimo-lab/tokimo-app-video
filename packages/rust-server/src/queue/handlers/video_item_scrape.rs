use sea_orm::DatabaseConnection;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;

use super::file_scrape;

/// Process all new/changed files for a single movie directory in one sequential job.
///
/// Payload schema:
/// ```json
/// {
///   "movieDir": "/path/to/MovieName (2024)",
///   "appId": "uuid", "sourceId": "uuid", "libType": "movie",
///   "files": [{ "filePath": "...", "dirPath": "...", "fileSize": 123, "checksum": "123:456" }]
/// }
/// ```
///
/// Sequential processing guarantees that the first file creates the movie record via TMDB,
/// and subsequent files (alternate versions, extras) find it in the DB and skip TMDB.
pub async fn handle(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    job_id: Uuid,
    payload: &JsonValue,
) -> Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>> {
    let movie_dir = payload
        .get("movieDir")
        .and_then(|v| v.as_str())
        .ok_or("Missing movieDir")?;
    let app_id = payload.get("appId").and_then(|v| v.as_str()).ok_or("Missing appId")?;
    let source_id =
        payload.get("sourceId").and_then(|v| v.as_str()).ok_or("Missing sourceId")?;
    let lib_type = payload.get("libType").and_then(|v| v.as_str()).ok_or("Missing libType")?;
    let files = payload
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or("Missing files array")?;

    let total = files.len();
    info!("[movie_scrape] dir=\"{movie_dir}\" files={total}");

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
                error!("[movie_scrape] Error on \"{file_path}\": {e}");
                errors += 1;
            }
        }
    }

    info!("[movie_scrape] dir=\"{movie_dir}\" done: {processed}/{total} ok, {errors} errors");

    Ok(Some(json!({
        "movieDir": movie_dir,
        "total": total,
        "processed": processed,
        "errors": errors,
    })))
}
