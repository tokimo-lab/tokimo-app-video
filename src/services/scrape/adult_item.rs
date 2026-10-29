//! Adult (R18) library item scraping: video ID → JavBus/JavDB/TPDB/StashDB metadata.

use sea_orm::prelude::Expr;
use sea_orm::*;
use serde_json::json;
use std::sync::Arc;
use tokimo_media_scraper::types::AdultMetadata;
use tracing::{info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::bus_clients::app_events;
use crate::db::entities::video_items;
use crate::services::common::{CastMember, is_unique_violation, sync_genres_from_names, sync_people_for_media};
use crate::services::nfo_parser::NfoInfo;
use crate::services::scrape::shared::adult::{self, AdultClients};
use crate::services::scrape::shared::artwork::{
    DiscoveredArtwork, upload_extra_art, upload_image_buffer, upload_poster_and_backdrop,
};
use crate::services::scrape::video_item::VideoItemResult;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Find or create an adult video item.
///
/// Existing records that were never scraped (e.g. created before adult scraping worked)
/// are enriched in place, so a library rescan repairs them.
#[allow(clippy::too_many_arguments)]
pub async fn find_or_create_adult_item(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    app_id: Uuid,
    nfo: Option<&NfoInfo>,
    parsed_title: &str,
    parsed_year: Option<i32>,
    artwork: &DiscoveredArtwork,
    user_id: Option<Uuid>,
) -> Result<VideoItemResult, BoxError> {
    let video_id = adult::extract_video_id(parsed_title)
        .or_else(|| nfo.and_then(|n| n.title.as_deref()).and_then(adult::extract_video_id));

    let existing = match video_id.as_deref() {
        Some(vid) => find_by_jav_number(db, app_id, vid).await?,
        None => None,
    };
    let existing = match existing {
        Some(m) => Some(m),
        None => find_by_title(db, app_id, parsed_title, parsed_year).await?,
    };

    if let Some(item) = &existing
        && item.scraped_at.is_some()
    {
        touch(db, item.id).await?;
        emit(state, app_id, item.id, "updated").await;
        return Ok(VideoItemResult {
            video_item_id: item.id,
            scraped: false,
        });
    }

    let clients = adult::load_clients(db, state).await;
    let meta = match (video_id.as_deref(), clients.as_ref()) {
        (Some(vid), Some(c)) => c.search(vid).await,
        _ => None,
    };
    match (&video_id, &meta) {
        (Some(vid), Some(m)) => info!("[adult_scrape] {vid} matched via {}", m.source),
        (Some(vid), None) if clients.is_some() => info!("[adult_scrape] {vid}: no metadata found"),
        (None, _) => info!("[adult_scrape] no video id in '{parsed_title}'"),
        _ => {}
    }

    let scraped = meta.is_some() || nfo.is_some_and(NfoInfo::is_sufficient);
    let (item_id, operation) = match existing {
        Some(item) => {
            update_record(db, item.id, video_id.as_deref(), meta.as_ref(), nfo, scraped).await?;
            (item.id, "updated")
        }
        None => {
            let id = insert_record(
                db,
                app_id,
                video_id.as_deref(),
                meta.as_ref(),
                nfo,
                parsed_title,
                parsed_year,
                scraped,
            )
            .await?;
            (id, "created")
        }
    };

    sync_artwork(db, state, item_id, artwork, meta.as_ref(), clients.as_deref()).await?;

    let genres = meta
        .as_ref()
        .and_then(|m| m.genres.clone())
        .filter(|g| !g.is_empty())
        .or_else(|| nfo.map(|n| n.genres.clone()))
        .unwrap_or_default();
    if !genres.is_empty() {
        sync_genres_from_names(db, &genres, Some(item_id), None).await?;
    }

    let cast: Vec<CastMember> = match meta.as_ref().and_then(|m| m.actors.as_ref()) {
        Some(actors) if !actors.is_empty() => actors
            .iter()
            .map(|name| CastMember {
                name: name.clone(),
                role: None,
                thumb: None,
                tmdb_id: None,
            })
            .collect(),
        _ => nfo
            .map(|n| {
                n.actors
                    .iter()
                    .map(|a| CastMember {
                        name: a.name.clone(),
                        role: a.role.clone(),
                        thumb: a.thumb.clone(),
                        tmdb_id: None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    let directors = nfo.map(|n| n.directors.clone()).unwrap_or_default();
    // No bus client: adult performers must not trigger TMDB person scrapes.
    sync_people_for_media(db, None, &cast, &directors, Some(item_id), None, None, user_id).await?;

    upload_extra_art(db, state, Some(item_id), None, &artwork.extra_art).await?;
    emit(state, app_id, item_id, operation).await;

    Ok(VideoItemResult {
        video_item_id: item_id,
        scraped,
    })
}

// ── Field mapping ────────────────────────────────────────────────────────────

struct AdultFields {
    title: Option<String>,
    year: Option<i32>,
    release_date: Option<chrono::NaiveDate>,
    runtime: Option<i32>,
    overview: Option<String>,
    javbus_id: Option<String>,
    javdb_id: Option<String>,
    metadata: Option<serde_json::Value>,
}

fn map_fields(video_id: Option<&str>, meta: Option<&AdultMetadata>, nfo: Option<&NfoInfo>) -> AdultFields {
    let release_date = meta
        .and_then(|m| m.release_date.as_deref())
        .or_else(|| nfo.and_then(|n| n.release_date.as_deref()))
        .and_then(|r| chrono::NaiveDate::parse_from_str(r, "%Y-%m-%d").ok());

    // Show "SSIS-001 片名" so the ID stays searchable in the library.
    let title = match (video_id, meta.and_then(|m| m.title.as_deref())) {
        (Some(vid), Some(t)) if !t.contains(vid) => Some(format!("{vid} {t}")),
        (_, Some(t)) => Some(t.to_string()),
        _ => nfo.and_then(|n| n.title.clone()),
    };

    let source_key = |source: &str| {
        meta.filter(|m| m.source == source)
            .and_then(|m| m.source_url.as_deref())
            .and_then(|u| u.trim_end_matches('/').rsplit('/').next())
            .map(str::to_string)
    };

    let mut metadata = serde_json::Map::new();
    if let Some(m) = meta {
        metadata.insert("adultSource".into(), json!(m.source));
        if let Some(u) = &m.source_url {
            metadata.insert("adultSourceUrl".into(), json!(u));
        }
        if let Some(r) = m.rating {
            metadata.insert("adultRating".into(), json!(r));
        }
    }
    if let Some(studio) = meta.and_then(|m| m.studio.clone()).or_else(|| nfo.and_then(|n| n.studio.clone())) {
        metadata.insert("studio".into(), json!(studio));
    }

    AdultFields {
        title,
        year: release_date.map(|d| chrono::Datelike::year(&d)),
        release_date,
        runtime: meta
            .and_then(|m| m.duration)
            .and_then(|d| i32::try_from(d).ok())
            .or_else(|| nfo.and_then(|n| n.runtime)),
        overview: nfo.and_then(|n| n.plot.clone()),
        javbus_id: source_key("javbus"),
        javdb_id: source_key("javdb"),
        metadata: (!metadata.is_empty()).then_some(serde_json::Value::Object(metadata)),
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_record(
    db: &DatabaseConnection,
    app_id: Uuid,
    video_id: Option<&str>,
    meta: Option<&AdultMetadata>,
    nfo: Option<&NfoInfo>,
    parsed_title: &str,
    parsed_year: Option<i32>,
    scraped: bool,
) -> Result<Uuid, BoxError> {
    let f = map_fields(video_id, meta, nfo);
    let now = chrono::Utc::now().fixed_offset();
    let id = Uuid::new_v4();
    let title = f
        .title
        .clone()
        .or_else(|| video_id.map(str::to_string))
        .unwrap_or_else(|| parsed_title.to_string());

    let build = |with_ids: bool| video_items::ActiveModel {
        id: Set(id),
        video_id: Set(app_id),
        title: Set(title.clone()),
        original_title: Set(None),
        sort_title: Set(None),
        year: Set(f.year.or(parsed_year)),
        release_date: Set(f.release_date),
        runtime: Set(f.runtime),
        tmdb_rating: Set(None),
        imdb_rating: Set(None),
        douban_rating: Set(None),
        tmdb_id: Set(None),
        imdb_id: Set(None),
        douban_id: Set(None),
        jav_number: Set(video_id.filter(|_| with_ids).map(str::to_string)),
        javbus_id: Set(f.javbus_id.clone().filter(|_| with_ids)),
        javdb_id: Set(f.javdb_id.clone().filter(|_| with_ids)),
        poster_path: Set(None),
        backdrop_path: Set(None),
        overview: Set(f.overview.clone()),
        tagline: Set(None),
        is_adult: Set(true),
        is_favorite: Set(false),
        original_language: Set(None),
        countries: Set(None),
        spoken_languages: Set(None),
        content_rating: Set(nfo.and_then(|n| n.content_rating.clone())),
        content_advisories: Set(None),
        locked_fields: Set(None),
        metadata: Set(f.metadata.clone()),
        scraped_at: Set(scraped.then_some(now)),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
    };

    match video_items::Entity::insert(build(true)).exec(db).await {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            // Concurrent sibling job in this library already created it.
            if let Some(vid) = video_id
                && let Some(m) = find_by_jav_number(db, app_id, vid).await?
            {
                return Ok(m.id);
            }
            // jav_number / javbus_id / javdb_id are globally unique: the same title
            // already lives in another library, so store this copy without them.
            warn!("[adult_scrape] {title}: external IDs taken by another library, inserting without them");
            video_items::Entity::insert(build(false)).exec(db).await?;
        }
        Err(e) => return Err(e.into()),
    }
    info!("[adult_scrape] Created adult item: {title}");
    Ok(id)
}

async fn update_record(
    db: &DatabaseConnection,
    item_id: Uuid,
    video_id: Option<&str>,
    meta: Option<&AdultMetadata>,
    nfo: Option<&NfoInfo>,
    scraped: bool,
) -> Result<(), BoxError> {
    let f = map_fields(video_id, meta, nfo);
    let mut update = video_items::Entity::update_many()
        .filter(video_items::Column::Id.eq(item_id))
        .col_expr(video_items::Column::IsAdult, Expr::value(true))
        .col_expr(video_items::Column::UpdatedAt, Expr::cust("NOW()"));
    if let Some(v) = f.title {
        update = update.col_expr(video_items::Column::Title, Expr::value(v));
    }
    if let Some(v) = f.year {
        update = update.col_expr(video_items::Column::Year, Expr::value(v));
    }
    if let Some(v) = f.release_date {
        update = update.col_expr(video_items::Column::ReleaseDate, Expr::value(v));
    }
    if let Some(v) = f.runtime {
        update = update.col_expr(video_items::Column::Runtime, Expr::value(v));
    }
    if let Some(v) = f.overview {
        update = update.col_expr(video_items::Column::Overview, Expr::value(v));
    }
    if let Some(v) = f.metadata {
        update = update.col_expr(video_items::Column::Metadata, Expr::value(v));
    }
    if scraped {
        update = update.col_expr(video_items::Column::ScrapedAt, Expr::cust("NOW()"));
    }
    update.exec(db).await?;

    // External IDs are globally unique; skip them if another library already owns them.
    let ids = [
        (video_items::Column::JavNumber, video_id.map(str::to_string)),
        (video_items::Column::JavbusId, f.javbus_id),
        (video_items::Column::JavdbId, f.javdb_id),
    ];
    for (col, value) in ids {
        let Some(value) = value else { continue };
        let res = video_items::Entity::update_many()
            .filter(video_items::Column::Id.eq(item_id))
            .col_expr(col, Expr::value(value))
            .exec(db)
            .await;
        match res {
            Ok(_) => {}
            Err(e) if is_unique_violation(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

// ── Artwork ──────────────────────────────────────────────────────────────────

/// Local artwork wins; otherwise download poster (vertical) and cover (horizontal) from the source.
async fn sync_artwork(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    item_id: Uuid,
    artwork: &DiscoveredArtwork,
    meta: Option<&AdultMetadata>,
    clients: Option<&AdultClients>,
) -> Result<(), BoxError> {
    let (mut poster_path, mut backdrop_path) =
        upload_poster_and_backdrop(state, "movie", item_id, artwork, None, None, None, None).await?;

    if let Some(meta) = meta
        && (poster_path.is_none() || backdrop_path.is_none())
    {
        let mut poster_buf = match &meta.poster_url {
            Some(u) => download(state, u).await,
            None => None,
        };
        if poster_buf.is_none()
            && let Some(thumb) = match clients {
                Some(c) => c.fetch_thumb_from_search(&meta.video_id).await,
                None => None,
            }
        {
            poster_buf = download(state, &thumb).await;
        }
        let cover_buf = match &meta.cover_url {
            Some(u) => download(state, u).await,
            None => None,
        };

        let prefix = format!("library-images/video_items/{item_id}");
        if poster_path.is_none()
            && let Some(buf) = &poster_buf
        {
            poster_path = Some(upload_image_buffer(state.storage(), buf, &format!("{prefix}/poster.jpg")).await?);
        }
        if backdrop_path.is_none()
            && let Some(buf) = cover_buf.as_ref().or(poster_buf.as_ref())
        {
            backdrop_path = Some(upload_image_buffer(state.storage(), buf, &format!("{prefix}/backdrop.jpg")).await?);
        }
    }

    if poster_path.is_some() || backdrop_path.is_some() {
        let mut update = video_items::Entity::update_many().filter(video_items::Column::Id.eq(item_id));
        if let Some(pp) = &poster_path {
            update = update.col_expr(video_items::Column::PosterPath, Expr::value(pp.as_str()));
        }
        if let Some(bp) = &backdrop_path {
            update = update.col_expr(video_items::Column::BackdropPath, Expr::value(bp.as_str()));
        }
        update.exec(db).await?;
    }
    Ok(())
}

async fn download(state: &AppState, url: &str) -> Option<Vec<u8>> {
    match crate::handlers::image_proxy::fetch_image(&state.http_client, url).await {
        Ok(bytes) if !bytes.is_empty() => Some(bytes.to_vec()),
        Ok(_) => None,
        Err(e) => {
            warn!("[adult_scrape] image download failed {url}: {e}");
            None
        }
    }
}

// ── Lookup helpers ───────────────────────────────────────────────────────────

async fn find_by_jav_number(
    db: &DatabaseConnection,
    app_id: Uuid,
    video_id: &str,
) -> Result<Option<video_items::Model>, BoxError> {
    Ok(video_items::Entity::find()
        .filter(video_items::Column::VideoId.eq(app_id))
        .filter(video_items::Column::JavNumber.eq(video_id))
        .one(db)
        .await?)
}

async fn find_by_title(
    db: &DatabaseConnection,
    app_id: Uuid,
    title: &str,
    year: Option<i32>,
) -> Result<Option<video_items::Model>, BoxError> {
    if title.is_empty() {
        return Ok(None);
    }
    let mut query = video_items::Entity::find()
        .filter(video_items::Column::VideoId.eq(app_id))
        .filter(video_items::Column::Title.eq(title));
    if let Some(y) = year {
        query = query.filter(video_items::Column::Year.eq(y));
    }
    Ok(query.one(db).await?)
}

async fn touch(db: &DatabaseConnection, item_id: Uuid) -> Result<(), BoxError> {
    video_items::Entity::update_many()
        .col_expr(video_items::Column::UpdatedAt, Expr::cust("NOW()"))
        .filter(video_items::Column::Id.eq(item_id))
        .exec(db)
        .await?;
    Ok(())
}

async fn emit(state: &Arc<AppState>, app_id: Uuid, item_id: Uuid, operation: &str) {
    if let Some(client) = state.bus_client.get() {
        let _ = app_events::emit_entity(
            client,
            "video_item",
            Some(format!("library:{app_id}")),
            json!({ "id": item_id.to_string(), "operation": operation, "libraryId": app_id.to_string() }),
        )
        .await;
    }
}
