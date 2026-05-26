use sea_orm::DatabaseConnection;
use serde_json::{Value as JsonValue, json};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::bus_clients::jobs;

use crate::queue::cancellation::{JobCancel, check_cancel};
use crate::queue::handlers::file_scrape;

/// Process all new/changed files for a single movie directory in one sequential job.
///
/// Payload schema:
/// ```json
/// {
///   "movieDir": "/path/to/MovieName (2024)",
///   "videoId": "uuid", "sourceId": "uuid", "libType": "movie",
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
    params: &JsonValue,
    cancel: &JobCancel,
    user_id: Option<Uuid>,
) -> Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>> {
    check_cancel(cancel)?;
    let movie_dir = params
        .get("movieDir")
        .and_then(|v| v.as_str())
        .ok_or("Missing movieDir")?;
    let video_id = params
        .get("videoId")
        .and_then(|v| v.as_str())
        .ok_or("Missing videoId")?;
    let source_id = params
        .get("sourceId")
        .and_then(|v| v.as_str())
        .ok_or("Missing sourceId")?;
    let lib_type = params
        .get("libType")
        .and_then(|v| v.as_str())
        .ok_or("Missing libType")?;
    let files = params
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or("Missing files array")?;

    let total = files.len();
    info!("[movie_scrape] dir=\"{movie_dir}\" files={total}");

    let mut processed = 0u32;
    let mut errors = 0u32;
    let mut last_reported_pct = -1;
    let mut last_reported_at: Option<Instant> = None;

    for file in files {
        check_cancel(cancel)?;
        let file_path = file.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
        let file_payload = json!({
            "filePath": file.get("filePath"),
            "dirPath": file.get("dirPath"),
            "fileSize": file.get("fileSize"),
            "checksum": file.get("checksum"),
            "videoId": video_id,
            "sourceId": source_id,
            "libType": lib_type,
        });

        match file_scrape::handle(db, state, job_id, &file_payload, cancel, user_id).await {
            Ok(_) => processed += 1,
            Err(e) => {
                error!("[movie_scrape] Error on \"{file_path}\": {e}");
                errors += 1;
            }
        }

        let current = processed + errors;
        report_progress(
            state,
            job_id,
            user_id,
            current,
            total,
            &mut last_reported_pct,
            &mut last_reported_at,
            format!("Scraping {current}/{total}: {movie_dir}"),
        )
        .await;
    }

    info!("[movie_scrape] dir=\"{movie_dir}\" done: {processed}/{total} ok, {errors} errors");

    if errors > 0 && processed == 0 {
        return Err(format!("all {errors} files failed").into());
    }

    Ok(Some(json!({
        "movieDir": movie_dir,
        "total": total,
        "processed": processed,
        "errors": errors,
    })))
}

async fn report_progress(
    state: &Arc<AppState>,
    job_id: Uuid,
    user_id: Option<Uuid>,
    current: u32,
    total: usize,
    last_reported_pct: &mut i32,
    last_reported_at: &mut Option<Instant>,
    label: String,
) {
    if total == 0 {
        return;
    }
    let pct = (((current as f64 / total as f64) * 100.0).round() as i32).clamp(0, 100);
    let is_final = (current as usize) >= total || pct == 100;
    if is_final && *last_reported_pct >= 100 {
        return;
    }
    let pct_changed = pct >= *last_reported_pct + 2;
    let time_elapsed = last_reported_at.is_none_or(|at| at.elapsed() >= Duration::from_millis(500));
    if !(pct_changed || time_elapsed || is_final) {
        return;
    }
    *last_reported_pct = pct;
    *last_reported_at = Some(Instant::now());

    let Some(client) = state.bus_client.get() else { return };
    jobs::update_progress(
        client,
        jobs::video_caller(user_id),
        job_id,
        pct,
        Some(json!({ "progress": { "current": current, "total": total, "label": label } })),
    )
    .await
    .ok();
}
