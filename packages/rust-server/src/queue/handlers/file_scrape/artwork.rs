//! Artwork upload helpers: local poster/backdrop, extra art, TMDB image job dispatch.

use bytes::Bytes;
use sea_orm::*;
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::db::entities::media_arts;
use crate::db::repos::job_repo::JobRepo;
use crate::services::storage::{StorageProvider, UploadOptions};
use crate::AppState;

use super::constants::{image_mime, image_storage_ext, EXTRA_ART, FANART_NAMES, POSTER_NAMES};
use super::parse::find_stem_poster_filename;
use super::DirContext;

/// Upload a local image buffer to S3 and return the storage path.
pub async fn upload_image_buffer(
    storage: &Arc<dyn StorageProvider>,
    buf: &[u8],
    storage_key: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let ext = storage_key.rsplit('.').next().unwrap_or("jpg");
    let mime = image_mime(ext);
    storage
        .upload(
            storage_key,
            Bytes::from(buf.to_vec()),
            Some(UploadOptions {
                content_type: Some(mime.to_string()),
            }),
        )
        .await
        .map_err(|e| format!("Storage upload failed: {e}"))?;
    Ok(format!("/storage/{storage_key}"))
}

/// Dispatch TMDB image_upload job.
pub async fn dispatch_tmdb_image_job(
    db: &DatabaseConnection,
    tmdb_path: &str,
    entity: &str,
    entity_id: &str,
    field: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Aligned with TS: always w500, key = tmdb-images/{entity}/{entityId}/{field}.{ext}
    let url = format!("https://image.tmdb.org/t/p/w500{tmdb_path}");
    let ext = tmdb_path.rsplit('.').next().unwrap_or("jpg");
    let storage_key = format!("tmdb-images/{entity}/{entity_id}/{field}.{ext}");
    JobRepo::create_job(
        db,
        "image_upload",
        json!({
            "plexUrl": url,
            "storageKey": storage_key,
            "entity": entity,
            "entityId": entity_id,
            "field": field,
        }),
        None,
    )
    .await?;
    Ok(())
}

pub struct DiscoveredArtwork {
    pub poster_buf: Option<Vec<u8>>,
    pub poster_filename: Option<String>,
    pub fanart_buf: Option<Vec<u8>>,
    pub extra_art: Vec<ExtraArtBuf>,
}

pub struct ExtraArtBuf {
    pub art_type: String,
    pub buf: Vec<u8>,
    pub ext: String,
}

/// Discover and read poster/fanart/extra art from directory via VFS.
pub async fn discover_artwork(ctx: &DirContext) -> DiscoveredArtwork {
    let dir_lower: Vec<String> = ctx.dir_entries.iter().map(|e| e.to_ascii_lowercase()).collect();

    // Find poster
    let poster_filename = POSTER_NAMES
        .iter()
        .find(|&&name| dir_lower.iter().any(|e| e == name))
        .map(|&s| s.to_string())
        .or_else(|| find_stem_poster_filename(&ctx.dir_entries, &ctx.stem));

    let poster_buf = match &poster_filename {
        Some(pf) => read_file_from_dir(ctx, pf).await,
        None => None,
    };

    // Find fanart
    let fanart_filename = FANART_NAMES.iter().find(|&&name| dir_lower.iter().any(|e| e == name));
    let fanart_buf = match fanart_filename {
        Some(&ff) => read_file_from_dir(ctx, ff).await,
        None => None,
    };

    // Find extra art
    let mut extra_art = Vec::new();
    for def in EXTRA_ART {
        let found = def.names.iter().find_map(|&name| {
            dir_lower
                .iter()
                .position(|e| e == name)
                .map(|idx| ctx.dir_entries[idx].clone())
        });
        if let Some(found) = found {
            if let Some(buf) = read_file_from_dir(ctx, &found).await {
                let ext = found.rsplit('.').next().unwrap_or("jpg").to_ascii_lowercase();
                extra_art.push(ExtraArtBuf {
                    art_type: def.art_type.to_string(),
                    buf,
                    ext,
                });
            }
        }
    }

    DiscoveredArtwork {
        poster_buf,
        poster_filename,
        fanart_buf,
        extra_art,
    }
}

async fn read_file_from_dir(ctx: &DirContext, filename: &str) -> Option<Vec<u8>> {
    let full_path = format!("{}/{}", ctx.dir_path.trim_end_matches('/'), filename);
    ctx.vfs
        .read_bytes(std::path::Path::new(&full_path), 0, None)
        .await
        .ok()
}

/// Upload poster and backdrop for a movie or TV show.
/// Returns (poster_storage_path, backdrop_storage_path) for local uploads.
#[allow(clippy::too_many_arguments)]
pub async fn upload_poster_and_backdrop(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    entity: &str,
    entity_id: Uuid,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb_path: Option<&str>,
    nfo_backdrop_tmdb_path: Option<&str>,
    tmdb_poster_path: Option<&str>,
    tmdb_backdrop_path: Option<&str>,
) -> Result<(Option<String>, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
    let id_str = entity_id.to_string();
    let folder = if entity == "movie" { "movies" } else { "tvshows" };
    let mut poster_storage_path: Option<String> = None;
    let mut backdrop_storage_path: Option<String> = None;

    // Poster: local → NFO TMDB URL → TMDB API
    if let (Some(buf), Some(filename)) = (&artwork.poster_buf, &artwork.poster_filename) {
        let ext = image_storage_ext(filename);
        let key = format!("library-images/{folder}/{id_str}/poster.{ext}");
        poster_storage_path = Some(upload_image_buffer(&state.storage, buf, &key).await?);
    } else {
        let tmdb_path = nfo_poster_tmdb_path.or(tmdb_poster_path);
        if let Some(path) = tmdb_path {
            dispatch_tmdb_image_job(db, path, entity, &id_str, "posterPath").await?;
        }
    }

    // Backdrop: local → NFO TMDB URL → TMDB API
    if let Some(buf) = &artwork.fanart_buf {
        let key = format!("library-images/{folder}/{id_str}/backdrop.jpg");
        backdrop_storage_path = Some(upload_image_buffer(&state.storage, buf, &key).await?);
    } else {
        let tmdb_path = nfo_backdrop_tmdb_path.or(tmdb_backdrop_path);
        if let Some(path) = tmdb_path {
            dispatch_tmdb_image_job(db, path, entity, &id_str, "backdropPath").await?;
        }
    }

    Ok((poster_storage_path, backdrop_storage_path))
}

/// Upload extra art to media_arts table.
pub async fn upload_extra_art(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    movie_id: Option<Uuid>,
    tv_show_id: Option<Uuid>,
    extra_art: &[ExtraArtBuf],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (folder, id_str) = if let Some(mid) = movie_id {
        ("movies", mid.to_string())
    } else if let Some(tid) = tv_show_id {
        ("tvshows", tid.to_string())
    } else {
        return Ok(());
    };

    for art in extra_art {
        let key = format!("library-images/{folder}/{id_str}/{}.{}", art.art_type, art.ext);
        let storage_path = upload_image_buffer(&state.storage, &art.buf, &key).await?;

        let model = media_arts::ActiveModel {
            id: Set(Uuid::new_v4()),
            movie_id: Set(movie_id),
            tv_show_id: Set(tv_show_id),
            season_id: Set(None),
            album_id: Set(None),
            novel_id: Set(None),
            art_type: Set(art.art_type.clone()),
            url: Set(storage_path),
            width: Set(None),
            height: Set(None),
            language: Set(None),
            source: Set(Some("local".to_string())),
            is_selected: Set(true),
            created_at: Set(chrono::Utc::now().fixed_offset()),
        };

        match media_arts::Entity::insert(model).exec(db).await {
            Ok(_) => {
                info!("[file_scrape] Uploaded extra art: {} for {}/{}", art.art_type, folder, id_str);
            }
            Err(e) => {
                warn!("[file_scrape] Failed to insert media_art: {e}");
            }
        }
    }

    Ok(())
}
