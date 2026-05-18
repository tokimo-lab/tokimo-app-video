//! `file_scrape` queue handler.
//!
//! Pipeline:
//!   parse payload → photo early-return → idempotency check →
//!   VFS listing → NFO → artwork → parse filename →
//!   match `lib_type` { `online_video` | movie/adult/custom | tv/anime } →
//!   create `media_file` → sync subtitles → dispatch ffprobe →
//!   capture episode screenshot (if no TMDB still)

use sea_orm::prelude::Expr;
use sea_orm::*;
use serde_json::{Value as JsonValue, json};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::db::entities::video_files;
use crate::queue::cancellation::{JobCancel, check_cancel};

use crate::services::nfo_parser::{self, NfoInfo, extract_tmdb_path};

use crate::services::scrape::shared::{
    DirContext,
    artwork::discover_artwork,
    episode_screenshot,
    lib_type::LibType,
    media_file,
    parse::{is_placeholder_disc_stem, parse_media_filename},
    subtitle::sync_subtitles,
};
use crate::services::scrape::{
    online_video::{fetch_online_record, find_or_create_online_video},
    tv, video_item,
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub async fn handle(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    _job_id: Uuid,
    payload: &JsonValue,
    cancel: &JobCancel,
) -> Result<Option<JsonValue>, BoxError> {
    check_cancel(cancel)?;
    // ── 0. Parse payload ──
    let file_path = payload
        .get("filePath")
        .and_then(|v| v.as_str())
        .ok_or("Missing filePath")?;
    let dir_path = payload
        .get("dirPath")
        .and_then(|v| v.as_str())
        .ok_or("Missing dirPath")?;
    let file_size = payload
        .get("fileSize")
        .and_then(sea_orm::JsonValue::as_i64)
        .unwrap_or(0);
    let checksum = payload.get("checksum").and_then(|v| v.as_str());
    let app_id = payload.get("appId").and_then(|v| v.as_str()).ok_or("Missing appId")?;
    let source_id = payload
        .get("sourceId")
        .and_then(|v| v.as_str())
        .ok_or("Missing sourceId")?;
    let lib_type_str = payload
        .get("libType")
        .and_then(|v| v.as_str())
        .ok_or("Missing libType")?;

    let app_uuid = Uuid::parse_str(app_id)?;
    let source_uuid = Uuid::parse_str(source_id)?;
    let lib_type = LibType::parse(lib_type_str)?;

    // ── 1. Photo early-return ──
    if lib_type == LibType::Photo {
        // blocker: photo::handle not available in video submodule — see video-blockers.md
        unimplemented!("blocker: file_scrape photo branch requires photo::handle which is not in video submodule scope");
    }

    // ── 2. Idempotency check ──
    let already_indexed = check_idempotency(db, source_uuid, file_path, app_uuid, &lib_type).await?;
    if let Some(indexed) = already_indexed {
        debug!("[file_scrape] Already indexed, skipping: {file_path}");
        media_file::try_nfo_patch(db, state, source_id, dir_path, lib_type, &indexed).await;
        return Ok(Some(json!({ "skipped": true, "reason": "already_ingested" })));
    }

    // ── 3. VFS + directory listing ──
    let vfs = state
        .sources
        .ensure_vfs(source_id)
        .await
        .map_err(|e| format!("Failed to get VFS for source {source_id}: {e}"))?;

    let filename = file_path.rsplit('/').next().unwrap_or(file_path);
    let stem = filename.rsplit_once('.').map_or(filename, |(n, _)| n).to_string();
    let dir_folder_name = dir_path.trim_end_matches('/').rsplit('/').next().unwrap_or("");

    let dir_entries: Vec<String> = match vfs.list(std::path::Path::new(dir_path)).await {
        Ok(entries) => entries.into_iter().map(|e| e.name).collect(),
        Err(e) => {
            warn!("[file_scrape] Failed to list directory {dir_path}: {e}");
            vec![]
        }
    };

    let ctx = DirContext {
        vfs: vfs.clone(),
        dir_path: dir_path.to_string(),
        dir_entries: dir_entries.clone(),
        stem: stem.clone(),
    };

    // ── 4. Read NFO ──
    let nfo = read_nfo(&ctx, &stem, dir_folder_name).await;

    // ── 5. Discover artwork ──
    let artwork = discover_artwork(&ctx).await;

    // ── 6. Parse filename + directory name ──
    let parsed_file = parse_media_filename(filename, Some(dir_folder_name));
    let parsed_dir = parse_media_filename(dir_folder_name, None);

    let parsed_grandparent = if lib_type.is_tv_family() && looks_like_season_folder(dir_folder_name) {
        dir_path
            .trim_end_matches('/')
            .rsplit('/')
            .nth(1)
            .map(|gp| parse_media_filename(gp, None))
            .filter(|gp| !gp.title.is_empty() && gp.title.len() >= 2)
    } else {
        None
    };

    let should_prefer_dir_title = !parsed_dir.title.is_empty()
        && parsed_dir.title.len() >= 2
        && (is_placeholder_disc_stem(filename, &parsed_file.title)
            || (parsed_dir.year.is_some() && parsed_dir.title.len() <= parsed_file.title.len()));

    let parsed_title_str = if let Some(ref gp) = parsed_grandparent {
        &gp.title
    } else if should_prefer_dir_title {
        &parsed_dir.title
    } else {
        &parsed_file.title
    };
    let parsed_year = parsed_grandparent
        .as_ref()
        .and_then(|gp| gp.year)
        .or(parsed_dir.year)
        .or(parsed_file.year);
    let parsed_season = parsed_file.season;
    let parsed_episodes = &parsed_file.episodes;

    let title = nfo
        .as_ref()
        .and_then(|n| n.title.as_deref())
        .unwrap_or(parsed_title_str);
    let year = nfo.as_ref().and_then(|n| n.year).or(parsed_year);

    info!("[file_scrape] Processing: {filename} -> title={title}, year={year:?}");

    let nfo_poster_tmdb = nfo.as_ref().and_then(|n| extract_tmdb_path(n.poster_url.as_deref()));
    let nfo_backdrop_tmdb = nfo.as_ref().and_then(|n| extract_tmdb_path(n.backdrop_url.as_deref()));

    // ── 7. Branch by lib_type ──
    let mut movie_id: Option<Uuid> = None;
    let mut tv_show_id: Option<Uuid> = None;
    let mut episode_id: Option<Uuid> = None;
    let mut scraped = false;
    let mut online_record = None;

    match lib_type {
        LibType::OnlineVideo => {
            online_record = fetch_online_record(db, app_uuid, dir_path).await?;
            scraped = nfo
                .as_ref()
                .is_some_and(NfoInfo::is_sufficient)
                || online_record.is_some();

            let result = find_or_create_online_video(
                db,
                state,
                app_uuid,
                nfo.as_ref(),
                online_record.as_ref(),
                title,
                &artwork,
                nfo_poster_tmdb.as_deref(),
                nfo_backdrop_tmdb.as_deref(),
            )
            .await?;
            movie_id = Some(result.video_item_id);
        }

        LibType::Movie | LibType::Adult | LibType::Custom => {
            let result = video_item::scrape(
                db,
                state,
                app_uuid,
                lib_type,
                &nfo,
                title,
                year,
                &artwork,
                &nfo_poster_tmdb,
                &nfo_backdrop_tmdb,
                parsed_title_str,
                parsed_year,
            )
            .await?;
            movie_id = Some(result.video_item_id);
            scraped = result.scraped;
        }

        LibType::Tv | LibType::Anime => {
            let first_episode = parsed_episodes.as_ref().and_then(|eps| eps.first().copied());
            let result = tv::scrape(
                db,
                state,
                app_uuid,
                lib_type,
                &nfo,
                title,
                year,
                parsed_season,
                first_episode,
                &artwork,
                &nfo_poster_tmdb,
                &nfo_backdrop_tmdb,
            )
            .await?;
            tv_show_id = Some(result.tv_show_id);
            episode_id = result.episode_id;
            scraped = true;
        }

        _ => {
            warn!("[file_scrape] Unknown lib_type '{lib_type_str}', skipping content record");
        }
    }

    // ── 8. Create or update MediaFile ──
    let media_file_id = media_file::create_or_update(
        db,
        source_uuid,
        app_uuid,
        file_path,
        filename,
        file_size,
        checksum,
        movie_id,
        episode_id,
        nfo.as_ref(),
        online_record.as_ref(),
        lib_type,
    )
    .await?;

    // ── 9. Sync subtitles ──
    sync_subtitles(db, state, media_file_id, &ctx).await?;

    // ── 10. Run FFprobe inline ──
    if let Err(e) = crate::queue::handlers::media_file_ffprobe::run_for_file(db, state, media_file_id, cancel).await {
        warn!("[file_scrape] FFprobe failed for {file_path}: {e}");
    }

    // ── 11. Capture episode still when TMDB had no image ──
    if let Some(eid) = episode_id {
        episode_screenshot::maybe_capture_episode_screenshot(db, state, eid, media_file_id, vfs.clone(), file_path)
            .await;
    }

    Ok(Some(json!({
        "filePath": file_path,
        "videoItemId": movie_id.map(|id| id.to_string()),
        "tvShowId": tv_show_id.map(|id| id.to_string()),
        "episodeId": episode_id.map(|id| id.to_string()),
        "mediaFileId": media_file_id.to_string(),
        "scraped": scraped,
    })))
}

// ── Local helpers ─────────────────────────────────────────────────────────────

async fn check_idempotency(
    db: &DatabaseConnection,
    source_uuid: Uuid,
    file_path: &str,
    app_uuid: Uuid,
    lib_type: &LibType,
) -> Result<Option<video_files::Model>, BoxError> {
    let mut query = video_files::Entity::find()
        .filter(video_files::Column::SourceId.eq(source_uuid))
        .filter(video_files::Column::Path.eq(file_path));

    if lib_type.is_movie_family() {
        query = query
            .filter(video_files::Column::VideoItemId.is_not_null())
            .filter(Expr::cust(format!(
                "EXISTS (SELECT 1 FROM video_items m WHERE m.id = video_files.video_item_id AND m.video_id = '{app_uuid}')"
            )));
    } else if lib_type.is_tv_family() {
        query = query
            .filter(video_files::Column::EpisodeId.is_not_null())
            .filter(Expr::cust(format!(
                "EXISTS (SELECT 1 FROM episodes e JOIN tv_shows ts ON e.tv_show_id = ts.id WHERE e.id = video_files.episode_id AND ts.video_id = '{app_uuid}')"
            )));
    }

    Ok(query.one(db).await?)
}

async fn read_nfo(ctx: &DirContext, stem: &str, dir_folder_name: &str) -> Option<NfoInfo> {
    let lower_entries: Vec<String> = ctx.dir_entries.iter().map(|e| e.to_ascii_lowercase()).collect();
    let stem_lower = stem.to_ascii_lowercase();
    let dir_lower = dir_folder_name.to_ascii_lowercase();

    let nfo_filename = lower_entries
        .iter()
        .position(|e| e == &format!("{stem_lower}.nfo"))
        .or_else(|| lower_entries.iter().position(|e| e == "movie.nfo"))
        .or_else(|| lower_entries.iter().position(|e| e == &format!("{dir_lower}.nfo")))
        .map(|idx| ctx.dir_entries[idx].clone());

    let nfo_filename = nfo_filename?;

    let full_path = format!("{}/{}", ctx.dir_path.trim_end_matches('/'), nfo_filename);
    let buf = ctx
        .vfs
        .read_bytes(std::path::Path::new(&full_path), 0, None)
        .await
        .ok()?;
    let content = String::from_utf8_lossy(&buf);
    Some(nfo_parser::parse_nfo(&content))
}

/// Returns true when a directory name looks like a season subfolder.
fn looks_like_season_folder(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("season") || lower.starts_with("specials") {
        return true;
    }
    if let Some(rest) = lower.strip_prefix('s')
        && !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_digit())
    {
        return true;
    }
    if name.contains('季') {
        return true;
    }
    false
}
