//! `VideoFile` record creation/update, and NFO patch for already-indexed files.

use sea_orm::prelude::Expr;
use sea_orm::*;
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::db::entities::{episodes, tv_shows, video_files, video_items};
use crate::queue::handlers::common;

use super::DirContext;
use super::artwork;
use super::constants::{POSTER_NAMES, guess_mime, image_storage_ext};
use super::lib_type::LibType;
use super::parse;
use crate::queue::handlers::nfo_parser::{self, NfoInfo, extract_tmdb_path};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Create a new `VideoFile` or re-associate an existing orphan.
#[allow(clippy::too_many_arguments)]
pub async fn create_or_update(
    db: &DatabaseConnection,
    source_uuid: Uuid,
    app_uuid: Uuid,
    file_path: &str,
    filename: &str,
    file_size: i64,
    checksum: Option<&str>,
    movie_id: Option<Uuid>,
    episode_id: Option<Uuid>,
    nfo: Option<&NfoInfo>,
    online_record: Option<&crate::db::entities::download_records::Model>,
    lib_type: LibType,
) -> Result<Uuid, BoxError> {
    // Match orphan (no association) OR same-library file
    let or_conditions = {
        let mut conds = vec!["(video_files.video_item_id IS NULL AND video_files.episode_id IS NULL)".to_string()];
        if movie_id.is_some() {
            conds.push(format!("EXISTS (SELECT 1 FROM video_items m WHERE m.id = video_files.video_item_id AND m.video_id = '{app_uuid}')"));
        }
        if episode_id.is_some() {
            conds.push(format!("EXISTS (SELECT 1 FROM episodes e JOIN tv_shows ts ON e.tv_show_id = ts.id WHERE e.id = video_files.episode_id AND ts.video_id = '{app_uuid}')"));
        }
        conds.join(" OR ")
    };

    let existing = video_files::Entity::find()
        .filter(video_files::Column::SourceId.eq(source_uuid))
        .filter(video_files::Column::Path.eq(file_path))
        .filter(Expr::cust(format!("({or_conditions})")))
        .one(db)
        .await?;

    if let Some(existing) = existing {
        let mut update = video_files::Entity::update_many().filter(video_files::Column::Id.eq(existing.id));
        if movie_id.is_some() {
            update = update.col_expr(video_files::Column::VideoItemId, Expr::value(movie_id));
        }
        if episode_id.is_some() {
            update = update.col_expr(video_files::Column::EpisodeId, Expr::value(episode_id));
        }
        if let Some(cs) = checksum {
            update = update.col_expr(video_files::Column::Checksum, Expr::value(cs));
        }
        update
            .col_expr(video_files::Column::ScannedAt, Expr::cust("NOW()"))
            .col_expr(video_files::Column::IsAvailable, Expr::value(true))
            .exec(db)
            .await?;
        return Ok(existing.id);
    }

    let video_file_id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();
    let mime_type = guess_mime(filename);

    let duration = nfo
        .and_then(|n| n.duration_in_seconds)
        .or_else(|| online_record.and_then(|r| r.duration_seconds));
    let video_codec = nfo.and_then(|n| n.video_codec.clone());
    let video_width = nfo.and_then(|n| n.video_width);
    let video_height = nfo.and_then(|n| n.video_height);
    let video_profile = nfo.and_then(|n| n.video_profile.clone());
    let hdr_type = nfo.and_then(|n| n.hdr_type.clone());

    let video_streams = nfo
        .and_then(|n| {
            if n.frame_rate.is_some() || n.video_bitrate.is_some() {
                Some(json!({ "frameRate": n.frame_rate, "bitrate": n.video_bitrate }))
            } else {
                None
            }
        })
        .or(Some(serde_json::Value::Null));
    let audio_streams = nfo
        .and_then(|n| {
            n.audio_codec.as_ref().map(|codec| {
                json!([{
                    "codec": codec,
                    "channels": n.audio_channels,
                    "language": n.audio_languages.first().unwrap_or(&"und".to_string()),
                }])
            })
        })
        .or(Some(serde_json::Value::Null));

    let _ = lib_type; // reserved for future per-type logic

    let model = video_files::ActiveModel {
        id: Set(video_file_id),
        source_id: Set(Some(source_uuid)),
        path: Set(file_path.to_string()),
        filename: Set(filename.to_string()),
        size: Set(if file_size > 0 { Some(file_size) } else { None }),
        mime_type: Set(mime_type),
        duration: Set(duration),
        checksum: Set(checksum.map(std::string::ToString::to_string)),
        video_codec: Set(video_codec),
        video_width: Set(video_width),
        video_height: Set(video_height),
        video_profile: Set(video_profile),
        hdr_type: Set(hdr_type),
        video_streams: Set(video_streams),
        audio_streams: Set(audio_streams),
        is_available: Set(true),
        scanned_at: Set(Some(now)),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        video_item_id: Set(movie_id),
        episode_id: Set(episode_id),
        ffprobe_raw: Set(None),
        iso_meta: Set(None),
    };

    match video_files::Entity::insert(model).exec(db).await {
        Ok(_) => {
            info!("[file_scrape] Created video file: {filename} ({video_file_id})");
            Ok(video_file_id)
        }
        Err(e) if common::is_unique_violation(&e) => {
            let existing = video_files::Entity::find()
                .filter(video_files::Column::SourceId.eq(source_uuid))
                .filter(video_files::Column::Path.eq(file_path))
                .one(db)
                .await?
                .ok_or_else(|| format!("Unique violation but no existing video_file for {file_path}"))?;
            let mut update = video_files::Entity::update_many().filter(video_files::Column::Id.eq(existing.id));
            if movie_id.is_some() {
                update = update.col_expr(video_files::Column::VideoItemId, Expr::value(movie_id));
            }
            if episode_id.is_some() {
                update = update.col_expr(video_files::Column::EpisodeId, Expr::value(episode_id));
            }
            update
                .col_expr(video_files::Column::ScannedAt, Expr::cust("NOW()"))
                .col_expr(video_files::Column::IsAvailable, Expr::value(true))
                .exec(db)
                .await?;
            info!("[file_scrape] Video file already exists (concurrent), updated: {filename}");
            Ok(existing.id)
        }
        Err(e) => Err(e.into()),
    }
}

/// NFO patch for already-indexed files: fill in missing tmdbId / poster.
/// Only patches null fields — never overwrites existing data.
pub async fn try_nfo_patch(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    source_id: &str,
    dir_path: &str,
    lib_type: LibType,
    indexed: &video_files::Model,
) {
    let mut needs_tmdb_id = false;
    let mut needs_poster = false;
    let mut entity_movie_id: Option<Uuid> = None;
    let mut entity_tv_show_id: Option<Uuid> = None;

    if lib_type.is_movie_family() {
        if let Some(mid) = indexed.video_item_id
            && let Ok(Some(movie)) = video_items::Entity::find_by_id(mid).one(db).await
        {
            needs_tmdb_id = movie.tmdb_id.is_none();
            needs_poster = movie.poster_path.is_none();
            entity_movie_id = Some(mid);
        }
    } else if lib_type.is_tv_family()
        && let Some(eid) = indexed.episode_id
        && let Ok(Some(ep)) = episodes::Entity::find_by_id(eid).one(db).await
        && let Ok(Some(show)) = tv_shows::Entity::find_by_id(ep.tv_show_id).one(db).await
    {
        needs_tmdb_id = show.tmdb_id.is_none();
        needs_poster = show.poster_path.is_none();
        entity_tv_show_id = Some(ep.tv_show_id);
    }

    if (!needs_tmdb_id && !needs_poster) || (entity_movie_id.is_none() && entity_tv_show_id.is_none()) {
        return;
    }

    let Ok(vfs) = state.sources.ensure_vfs(source_id).await else {
        return;
    };
    let filename = indexed.filename.as_str();
    let stem = filename.rsplit_once('.').map_or(filename, |(n, _)| n).to_string();
    let dir_folder_name = dir_path.trim_end_matches('/').rsplit('/').next().unwrap_or("");

    let dir_entries: Vec<String> = match vfs.list(std::path::Path::new(dir_path)).await {
        Ok(entries) => entries.into_iter().map(|e| e.name).collect(),
        Err(_) => return,
    };

    let ctx = DirContext {
        vfs: vfs.clone(),
        dir_path: dir_path.to_string(),
        dir_entries: dir_entries.clone(),
        stem: stem.clone(),
    };

    let nfo = read_nfo_for_patch(&ctx, &stem, dir_folder_name).await;

    let dir_lower: Vec<String> = dir_entries.iter().map(|e| e.to_ascii_lowercase()).collect();
    // Prefer stem-matched poster over directory-level generic names
    let poster_filename = parse::find_stem_poster_filename(&dir_entries, &stem).or_else(|| {
        POSTER_NAMES
            .iter()
            .find(|&&name| dir_lower.iter().any(|e| e == name))
            .map(|&s| s.to_string())
    });
    let poster_buf = match &poster_filename {
        Some(pf) => {
            let path = format!("{}/{}", dir_path.trim_end_matches('/'), pf);
            vfs.read_bytes(std::path::Path::new(&path), 0, None).await.ok()
        }
        None => None,
    };

    if nfo.is_none() && poster_buf.is_none() {
        return;
    }

    if let Some(mid) = entity_movie_id {
        patch_movie(
            db,
            state,
            mid,
            needs_tmdb_id,
            needs_poster,
            &nfo,
            &poster_buf,
            &poster_filename,
        )
        .await;
    } else if let Some(tid) = entity_tv_show_id {
        patch_tv_show(
            db,
            state,
            tid,
            needs_tmdb_id,
            needs_poster,
            &nfo,
            &poster_buf,
            &poster_filename,
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn patch_movie(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    mid: Uuid,
    needs_tmdb_id: bool,
    needs_poster: bool,
    nfo: &Option<nfo_parser::NfoInfo>,
    poster_buf: &Option<Vec<u8>>,
    poster_filename: &Option<String>,
) {
    if needs_tmdb_id
        && let Some(nfo) = nfo
        && (nfo.tmdb_id.is_some() || nfo.imdb_id.is_some())
    {
        let mut update = video_items::Entity::update_many().filter(video_items::Column::Id.eq(mid));
        if let Some(ref tid) = nfo.tmdb_id {
            update = update.col_expr(video_items::Column::TmdbId, Expr::value(tid.as_str()));
        }
        if let Some(ref iid) = nfo.imdb_id {
            update = update.col_expr(video_items::Column::ImdbId, Expr::value(iid.as_str()));
        }
        match update.exec(db).await {
            Ok(_) => info!(
                "[nfo_patch] movie {mid}: set tmdbId={} imdbId={}",
                nfo.tmdb_id.as_deref().unwrap_or("-"),
                nfo.imdb_id.as_deref().unwrap_or("-")
            ),
            Err(e) => warn!("[nfo_patch] movie {mid}: failed to update tmdb/imdb id: {e}"),
        }
    }
    if needs_poster {
        if let (Some(buf), Some(pf)) = (poster_buf, poster_filename) {
            let ext = image_storage_ext(pf);
            let key = format!("library-images/movies/{mid}/poster.{ext}");
            if let Ok(sp) = artwork::upload_image_buffer(&state.storage, buf, &key).await {
                if let Err(e) = video_items::Entity::update_many()
                    .col_expr(video_items::Column::PosterPath, Expr::value(sp.as_str()))
                    .filter(video_items::Column::Id.eq(mid))
                    .exec(db)
                    .await
                {
                    warn!("[nfo_patch] movie {mid}: failed to set poster_path: {e}");
                } else {
                    info!("[nfo_patch] movie {mid}: uploaded poster {pf}");
                }
            }
        } else if let Some(tmdb_path) = nfo.as_ref().and_then(|n| extract_tmdb_path(n.poster_url.as_deref())) {
            match artwork::dispatch_tmdb_image_job(db, &tmdb_path, "movie", &mid.to_string(), "posterPath").await {
                Ok(()) => info!("[nfo_patch] movie {mid}: dispatched poster job from NFO"),
                Err(e) => warn!("[nfo_patch] movie {mid}: failed to dispatch poster job from NFO: {e}"),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn patch_tv_show(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    tid: Uuid,
    needs_tmdb_id: bool,
    needs_poster: bool,
    nfo: &Option<nfo_parser::NfoInfo>,
    poster_buf: &Option<Vec<u8>>,
    poster_filename: &Option<String>,
) {
    if needs_tmdb_id
        && let Some(nfo) = nfo
        && (nfo.tmdb_id.is_some() || nfo.imdb_id.is_some())
    {
        let mut update = tv_shows::Entity::update_many().filter(tv_shows::Column::Id.eq(tid));
        if let Some(ref tmdb) = nfo.tmdb_id {
            update = update.col_expr(tv_shows::Column::TmdbId, Expr::value(tmdb.as_str()));
        }
        if let Some(ref imdb) = nfo.imdb_id {
            update = update.col_expr(tv_shows::Column::ImdbId, Expr::value(imdb.as_str()));
        }
        match update.exec(db).await {
            Ok(_) => info!(
                "[nfo_patch] tvShow {tid}: set tmdbId={} imdbId={}",
                nfo.tmdb_id.as_deref().unwrap_or("-"),
                nfo.imdb_id.as_deref().unwrap_or("-")
            ),
            Err(e) => warn!("[nfo_patch] tvShow {tid}: failed to update tmdb/imdb id: {e}"),
        }
    }
    if needs_poster {
        if let (Some(buf), Some(pf)) = (poster_buf, poster_filename) {
            let ext = image_storage_ext(pf);
            let key = format!("library-images/tvshows/{tid}/poster.{ext}");
            if let Ok(sp) = artwork::upload_image_buffer(&state.storage, buf, &key).await {
                if let Err(e) = tv_shows::Entity::update_many()
                    .col_expr(tv_shows::Column::PosterPath, Expr::value(sp.as_str()))
                    .filter(tv_shows::Column::Id.eq(tid))
                    .exec(db)
                    .await
                {
                    warn!("[nfo_patch] tvShow {tid}: failed to set poster_path: {e}");
                } else {
                    info!("[nfo_patch] tvShow {tid}: uploaded poster {pf}");
                }
            }
        } else if let Some(tmdb_path) = nfo.as_ref().and_then(|n| extract_tmdb_path(n.poster_url.as_deref())) {
            match artwork::dispatch_tmdb_image_job(db, &tmdb_path, "tvShow", &tid.to_string(), "posterPath").await {
                Ok(()) => info!("[nfo_patch] tvShow {tid}: dispatched poster job from NFO"),
                Err(e) => warn!("[nfo_patch] tvShow {tid}: failed to dispatch poster job from NFO: {e}"),
            }
        }
    }
}

/// Read NFO file from a directory context for `nfo_patch`.
async fn read_nfo_for_patch(ctx: &DirContext, stem: &str, dir_folder_name: &str) -> Option<NfoInfo> {
    let lower_entries: Vec<String> = ctx.dir_entries.iter().map(|e| e.to_ascii_lowercase()).collect();
    let stem_lower = stem.to_ascii_lowercase();
    let dir_lower_name = dir_folder_name.to_ascii_lowercase();

    let nfo_filename = lower_entries
        .iter()
        .position(|e| e == &format!("{stem_lower}.nfo"))
        .or_else(|| lower_entries.iter().position(|e| e == "movie.nfo"))
        .or_else(|| lower_entries.iter().position(|e| e == &format!("{dir_lower_name}.nfo")))
        .map(|i| ctx.dir_entries[i].clone())?;

    let nfo_path = format!("{}/{}", ctx.dir_path.trim_end_matches('/'), nfo_filename);
    let bytes = ctx
        .vfs
        .read_bytes(std::path::Path::new(&nfo_path), 0, None)
        .await
        .ok()?;
    let content = String::from_utf8_lossy(&bytes).to_string();
    Some(nfo_parser::parse_nfo(&content))
}
