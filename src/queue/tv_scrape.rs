use sea_orm::DatabaseConnection;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;

use crate::queue::handlers::file_scrape;

/// Process all new/changed files for a single TV show.
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
/// The first file is processed sequentially to ensure the TV show record + first
/// season exist in DB (with exactly one TMDB API call). Remaining files are then
/// processed concurrently in batches, reusing the existing show/season records.
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

    if total == 0 {
        return Ok(Some(json!({
            "showDir": show_dir, "total": 0, "processed": 0, "errors": 0,
        })));
    }

    let processed = Arc::new(AtomicU32::new(0));
    let errors = Arc::new(AtomicU32::new(0));

    // Process the first file sequentially to create the show + first season via TMDB.
    {
        let file = &files[0];
        let file_path = file.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
        let file_payload = make_file_payload(file, app_id, source_id, lib_type);
        match file_scrape::handle(db, state, job_id, &file_payload).await {
            Ok(_) => { processed.fetch_add(1, Ordering::Relaxed); }
            Err(e) => {
                error!("[tv_scrape] Error on \"{file_path}\": {e}");
                errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    // Process remaining files concurrently in batches.
    const CONCURRENCY: usize = 8;
    let remaining = &files[1..];
    for chunk in remaining.chunks(CONCURRENCY) {
        let mut handles = Vec::with_capacity(chunk.len());
        for file in chunk {
            let file_payload = make_file_payload(file, app_id, source_id, lib_type);
            let db = db.clone();
            let state = state.clone();
            let processed = processed.clone();
            let errors = errors.clone();
            handles.push(tokio::spawn(async move {
                let file_path = file_payload.get("filePath").and_then(|v| v.as_str()).unwrap_or("").to_string();
                match file_scrape::handle(&db, &state, job_id, &file_payload).await {
                    Ok(_) => { processed.fetch_add(1, Ordering::Relaxed); }
                    Err(e) => {
                        error!("[tv_scrape] Error on \"{file_path}\": {e}");
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }));
        }
        for h in handles {
            let _ = h.await;
        }
    }

    let p = processed.load(Ordering::Relaxed);
    let e = errors.load(Ordering::Relaxed);
    info!("[tv_scrape] show=\"{show_dir}\" done: {p}/{total} ok, {e} errors");

    Ok(Some(json!({
        "showDir": show_dir,
        "total": total,
        "processed": p,
        "errors": e,
    })))
}

fn make_file_payload(file: &JsonValue, app_id: &str, source_id: &str, lib_type: &str) -> JsonValue {
    json!({
        "filePath": file.get("filePath"),
        "dirPath": file.get("dirPath"),
        "fileSize": file.get("fileSize"),
        "checksum": file.get("checksum"),
        "appId": app_id,
        "sourceId": source_id,
        "libType": lib_type,
    })
}
