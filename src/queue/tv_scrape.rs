use sea_orm::DatabaseConnection;
use serde_json::{Value as JsonValue, json};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::bus_clients::jobs;

use crate::queue::cancellation::{JobCancel, check_cancel};
use crate::queue::handlers::file_scrape;
use crate::services::nfo_parser::{self, NfoInfo, extract_tmdb_path};
use crate::services::scrape::shared::{
    DirContext, artwork::discover_artwork, lib_type::LibType, parse::parse_media_filename,
};
use crate::services::scrape::tv;

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

    let (show_title, show_year) = prepare_show(db, state, video_id, source_id, lib_type, show_dir, user_id).await?;

    let processed = Arc::new(AtomicU32::new(0));
    let errors = Arc::new(AtomicU32::new(0));
    let mut last_reported_pct = -1;
    let mut last_reported_at: Option<Instant> = None;

    // Process the first file sequentially to create the show + first season via TMDB.
    {
        let file = &files[0];
        let file_path = file.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
        let file_payload = make_file_payload(file, video_id, source_id, lib_type, &show_title, show_year);
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
            let file_payload = make_file_payload(file, video_id, source_id, lib_type, &show_title, show_year);
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

async fn prepare_show(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    video_id: &str,
    source_id: &str,
    lib_type: &str,
    show_dir: &str,
    user_id: Option<Uuid>,
) -> Result<(String, Option<i32>), Box<dyn std::error::Error + Send + Sync>> {
    let vfs = state
        .sources
        .ensure_vfs(source_id)
        .await
        .map_err(|e| format!("Failed to get VFS for source {source_id}: {e}"))?;
    let dir_entries = match vfs.list(Path::new(show_dir)).await {
        Ok(entries) => entries.into_iter().map(|entry| entry.name).collect(),
        Err(error) => {
            warn!("[tv_scrape] Failed to list show directory {show_dir}: {error}");
            Vec::new()
        }
    };
    let folder_name = show_dir.trim_end_matches('/').rsplit('/').next().unwrap_or(show_dir);
    let ctx = DirContext {
        vfs,
        dir_path: show_dir.to_string(),
        dir_entries,
        stem: folder_name.to_string(),
    };
    let show_nfo = read_show_nfo(&ctx, folder_name).await;
    let artwork = discover_artwork(&ctx).await;
    let parsed = parse_media_filename(folder_name, None);
    let show_title = show_nfo
        .as_ref()
        .and_then(|nfo| nfo.title.as_deref())
        .filter(|title| !title.is_empty())
        .unwrap_or(&parsed.title)
        .to_string();
    let show_year = show_nfo.as_ref().and_then(|nfo| nfo.year).or(parsed.year);
    let nfo_poster_tmdb = show_nfo
        .as_ref()
        .and_then(|nfo| extract_tmdb_path(nfo.poster_url.as_deref()));
    let nfo_backdrop_tmdb = show_nfo
        .as_ref()
        .and_then(|nfo| extract_tmdb_path(nfo.backdrop_url.as_deref()));

    tv::scrape(
        db,
        state,
        Uuid::parse_str(video_id)?,
        LibType::parse(lib_type)?,
        &show_nfo,
        &show_title,
        show_year,
        None,
        None,
        &artwork,
        &nfo_poster_tmdb,
        &nfo_backdrop_tmdb,
        user_id,
    )
    .await?;

    info!(
        "[tv_scrape] prepared show=\"{show_title}\" year={show_year:?} root_nfo={} root_poster={}",
        show_nfo.is_some(),
        artwork.poster_buf.is_some()
    );
    Ok((show_title, show_year))
}

async fn read_show_nfo(ctx: &DirContext, folder_name: &str) -> Option<NfoInfo> {
    let filename = select_show_nfo_filename(&ctx.dir_entries, folder_name)?;
    let full_path = format!("{}/{}", ctx.dir_path.trim_end_matches('/'), filename);
    let bytes = ctx.vfs.read_bytes(Path::new(&full_path), 0, None).await.ok()?;
    Some(nfo_parser::parse_nfo(&String::from_utf8_lossy(&bytes)))
}

fn select_show_nfo_filename(entries: &[String], folder_name: &str) -> Option<String> {
    let folder_nfo = format!("{}.nfo", folder_name.to_ascii_lowercase());
    entries
        .iter()
        .find(|entry| entry.eq_ignore_ascii_case("tvshow.nfo"))
        .or_else(|| entries.iter().find(|entry| entry.to_ascii_lowercase() == folder_nfo))
        .cloned()
}

fn make_file_payload(
    file: &JsonValue,
    video_id: &str,
    source_id: &str,
    lib_type: &str,
    show_title: &str,
    show_year: Option<i32>,
) -> JsonValue {
    json!({
        "filePath": file.get("filePath"),
        "dirPath": file.get("dirPath"),
        "fileSize": file.get("fileSize"),
        "checksum": file.get("checksum"),
        "videoId": video_id,
        "sourceId": source_id,
        "libType": lib_type,
        "showTitle": show_title,
        "showYear": show_year,
    })
}

#[cfg(test)]
mod tests {
    use super::select_show_nfo_filename;

    #[test]
    fn tvshow_nfo_takes_priority_over_folder_named_nfo() {
        let entries = vec!["Show Name.nfo".to_string(), "TVSHOW.NFO".to_string()];
        assert_eq!(
            select_show_nfo_filename(&entries, "Show Name").as_deref(),
            Some("TVSHOW.NFO")
        );
    }

    #[test]
    fn folder_named_nfo_is_supported_when_tvshow_nfo_is_absent() {
        let entries = vec!["Show Name.NFO".to_string()];
        assert_eq!(
            select_show_nfo_filename(&entries, "Show Name").as_deref(),
            Some("Show Name.NFO")
        );
    }
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
