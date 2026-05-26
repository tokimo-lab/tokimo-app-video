use sea_orm::DatabaseConnection;
use serde_json::{Value as JsonValue, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::bus_clients::jobs;

use crate::queue::cancellation::{JobCancel, check_cancel};
use crate::queue::handlers::file_scrape;

/// Process all new/changed files for a single TV show.
///
/// Payload schema:
/// ```json
/// {
///   "showDir": "/path/to/ShowName",
///   "videoId": "uuid", "sourceId": "uuid", "libType": "tv",
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
    params: &JsonValue,
    cancel: &JobCancel,
    user_id: Option<Uuid>,
) -> Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>> {
    check_cancel(cancel)?;
    let show_dir = params
        .get("showDir")
        .and_then(|v| v.as_str())
        .ok_or("Missing showDir")?;
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
    info!("[tv_scrape] show=\"{show_dir}\" files={total}");

    if total == 0 {
        return Ok(Some(json!({
            "showDir": show_dir, "total": 0, "processed": 0, "errors": 0,
        })));
    }

    let processed = Arc::new(AtomicU32::new(0));
    let errors = Arc::new(AtomicU32::new(0));
    let mut last_reported_pct = -1;
    let mut last_reported_at: Option<Instant> = None;

    // Process the first file sequentially to create the show + first season via TMDB.
    {
        let file = &files[0];
        let file_path = file.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
        let file_payload = make_file_payload(file, video_id, source_id, lib_type);
        match file_scrape::handle(db, state, job_id, &file_payload, cancel, user_id).await {
            Ok(_) => {
                processed.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                error!("[tv_scrape] Error on \"{file_path}\": {e}");
                errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        let current = processed.load(Ordering::Relaxed) + errors.load(Ordering::Relaxed);
        report_progress(
            state,
            job_id,
            user_id,
            current,
            total,
            &mut last_reported_pct,
            &mut last_reported_at,
            format!("Scraping {current}/{total}: {show_dir}"),
        )
        .await;
    }

    // Process remaining files concurrently in batches.
    const CONCURRENCY: usize = 8;
    let remaining = &files[1..];
    for chunk in remaining.chunks(CONCURRENCY) {
        check_cancel(cancel)?;
        let mut handles = Vec::with_capacity(chunk.len());
        for file in chunk {
            let file_payload = make_file_payload(file, video_id, source_id, lib_type);
            let db = db.clone();
            let state = state.clone();
            let processed = processed.clone();
            let errors = errors.clone();
            let cancel = cancel.clone();
            handles.push(tokio::spawn(async move {
                let file_path = file_payload
                    .get("filePath")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                match file_scrape::handle(&db, &state, job_id, &file_payload, &cancel, user_id).await {
                    Ok(_) => {
                        processed.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        error!("[tv_scrape] Error on \"{file_path}\": {e}");
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }));
        }
        for h in handles {
            let _ = h.await;
            let current = processed.load(Ordering::Relaxed) + errors.load(Ordering::Relaxed);
            report_progress(
                state,
                job_id,
                user_id,
                current,
                total,
                &mut last_reported_pct,
                &mut last_reported_at,
                format!("Scraping {current}/{total}: {show_dir}"),
            )
            .await;
        }
    }

    let p = processed.load(Ordering::Relaxed);
    let e = errors.load(Ordering::Relaxed);
    info!("[tv_scrape] show=\"{show_dir}\" done: {p}/{total} ok, {e} errors");

    if e > 0 && p == 0 {
        return Err(format!("all {e} files failed").into());
    }

    Ok(Some(json!({
        "showDir": show_dir,
        "total": total,
        "processed": p,
        "errors": e,
    })))
}

fn make_file_payload(file: &JsonValue, video_id: &str, source_id: &str, lib_type: &str) -> JsonValue {
    json!({
        "filePath": file.get("filePath"),
        "dirPath": file.get("dirPath"),
        "fileSize": file.get("fileSize"),
        "checksum": file.get("checksum"),
        "videoId": video_id,
        "sourceId": source_id,
        "libType": lib_type,
    })
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
