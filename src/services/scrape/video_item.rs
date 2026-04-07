//! Movie creation and lookup logic aligned with TS file-scrape.ts.

use rust_client_api::metadata_providers::tmdb::TmdbMediaDetail;
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde_json::json;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use crate::db::entities::video_items;
use crate::AppState;

use crate::services::media::scrape::shared::artwork::{upload_extra_art, upload_poster_and_backdrop, DiscoveredArtwork};
use crate::queue::handlers::common::{is_unique_violation, sync_genres, sync_genres_from_names, sync_people_for_media, CastMember};
use crate::services::media::scrape::shared::lib_type::LibType;
use crate::queue::handlers::nfo_parser::NfoInfo;
use crate::services::media::scrape::shared::tmdb;

pub struct VideoItemResult {
    pub video_item_id: Uuid,
    pub scraped: bool,
}

/// Single entry point called from mod.rs.
#[allow(clippy::too_many_arguments)]
pub async fn scrape(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    app_id: Uuid,
    lib_type: LibType,
    nfo: &Option<NfoInfo>,
    title: &str,
    year: Option<i32>,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb: &Option<String>,
    nfo_backdrop_tmdb: &Option<String>,
    parsed_title: &str,
    parsed_year: Option<i32>,
) -> Result<VideoItemResult, Box<dyn std::error::Error + Send + Sync>> {
    find_or_create_video_item(
        db, state, app_id, lib_type,
        nfo.as_ref(), parsed_title, parsed_year,
        title, year, artwork,
        nfo_poster_tmdb.as_deref(), nfo_backdrop_tmdb.as_deref(),
    )
    .await
}

/// Find or create a movie record with lazy TMDB resolution.
///
/// Steps:
/// 1. Check DB by NFO external IDs — return immediately if found (no TMDB call).
/// 2. Check DB by parsed title+year — return immediately if found (handles group_key
///    siblings: second video file in the same movie directory, where the first job
///    already created the record).
/// 3. Call TMDB — only reached when the movie is genuinely new.
/// 4. Re-check DB by TMDB-resolved IDs to handle rare cross-directory races.
/// 5. Insert the new movie record.
#[allow(clippy::too_many_arguments)]
pub async fn find_or_create_video_item(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    app_id: Uuid,
    lib_type: LibType,
    nfo: Option<&NfoInfo>,
    parsed_title: &str,
    parsed_year: Option<i32>,
    display_title: &str,
    display_year: Option<i32>,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb_path: Option<&str>,
    nfo_backdrop_tmdb_path: Option<&str>,
) -> Result<VideoItemResult, Box<dyn std::error::Error + Send + Sync>> {
    let should_use_tmdb = lib_type.uses_tmdb();
    let is_adult = lib_type.is_adult();

    // Step 1: Fast path — check by external IDs from NFO (no network call needed).
    let nfo_tmdb_id = nfo.and_then(|n| n.tmdb_id.as_deref());
    let nfo_imdb_id = nfo.and_then(|n| n.imdb_id.as_deref());
    if let Some(id) = find_existing_video_item(db, app_id, nfo_tmdb_id, nfo_imdb_id).await? {
        touch_video_item(db, id).await?;
        return Ok(VideoItemResult { video_item_id: id, scraped: false });
    }

    // Step 2: Check by parsed title+year — handles the common case where a sibling job
    // in the same movie directory already created the movie record.
    if let Some(id) = find_existing_video_item_by_title(db, app_id, parsed_title, parsed_year).await? {
        touch_video_item(db, id).await?;
        return Ok(VideoItemResult { video_item_id: id, scraped: false });
    }

    // Step 3: Movie not in DB — call TMDB now.
    let tmdb_detail = if should_use_tmdb {
        if let Some(api_key) = tmdb::get_api_key(db).await? {
            let client = tmdb::build_client(state, &api_key);
            tmdb::scrape_movie(
                &client, nfo, display_title, display_year, artwork,
                nfo_poster_tmdb_path, nfo_backdrop_tmdb_path,
            )
            .await
        } else {
            None
        }
    } else {
        None
    };

    let scraped =
        tmdb_detail.is_some() || nfo.is_some_and(crate::queue::handlers::nfo_parser::NfoInfo::is_sufficient);

    let tmdb_id_str = tmdb_detail
        .as_ref()
        .map(|d| d.base.id.to_string())
        .or_else(|| nfo.and_then(|n| n.tmdb_id.clone()));
    let imdb_id_str = tmdb_detail
        .as_ref()
        .and_then(|d| d.imdb_id.clone())
        .or_else(|| nfo.and_then(|n| n.imdb_id.clone()));

    // Step 4: Re-check by TMDB-resolved IDs to handle rare cross-directory races
    // (e.g. same movie appearing in two different libraries simultaneously).
    if let Some(id) = find_existing_video_item(db, app_id, tmdb_id_str.as_deref(), imdb_id_str.as_deref()).await? {
        backfill_video_item_ids(db, id, tmdb_detail.as_ref(), tmdb_id_str.as_deref(), imdb_id_str.as_deref()).await?;
        touch_video_item(db, id).await?;
        return Ok(VideoItemResult { video_item_id: id, scraped: false });
    }

    // Step 5: Create the movie record.
    let movie_id = create_video_item_record(
        db, app_id, is_adult, should_use_tmdb, tmdb_detail.as_ref(), nfo,
        parsed_title, parsed_year, tmdb_id_str.as_deref(), imdb_id_str.as_deref(), lib_type,
    )
    .await?;

    // Upload artwork.
    let (poster_path, backdrop_path) = upload_poster_and_backdrop(
        db, state, "movie", movie_id, artwork,
        nfo_poster_tmdb_path, nfo_backdrop_tmdb_path,
        tmdb_detail.as_ref().and_then(|d| d.base.poster_path.as_deref()),
        tmdb_detail.as_ref().and_then(|d| d.base.backdrop_path.as_deref()),
    )
    .await?;

    if poster_path.is_some() || backdrop_path.is_some() {
        let mut update = video_items::Entity::update_many().filter(video_items::Column::Id.eq(movie_id));
        if let Some(pp) = &poster_path {
            update = update.col_expr(video_items::Column::PosterPath, Expr::value(pp.as_str()));
        }
        if let Some(bp) = &backdrop_path {
            update = update.col_expr(video_items::Column::BackdropPath, Expr::value(bp.as_str()));
        }
        update.exec(db).await?;
    }

    // Sync genres (TMDB preferred, NFO fallback).
    if let Some(detail) = tmdb_detail.as_ref() {
        if let Some(genres) = &detail.genres {
            sync_genres(db, genres, Some(movie_id), None).await?;
        }
    } else if let Some(nfo) = nfo
        && !nfo.genres.is_empty() {
            sync_genres_from_names(db, &nfo.genres, Some(movie_id), None).await?;
        }

    // Sync cast/people: unified approach (TMDB cast preferred, NFO actors fallback).
    {
        let cast: Vec<CastMember> = if let Some(detail) = tmdb_detail.as_ref() {
            detail.cast.as_deref().unwrap_or(&[]).iter().map(CastMember::from).collect()
        } else if let Some(nfo) = nfo {
            nfo.actors.iter().map(|a| CastMember {
                name: a.name.clone(),
                role: a.role.clone(),
                thumb: a.thumb.clone(),
                tmdb_id: None,
            }).collect()
        } else {
            vec![]
        };
        let directors: Vec<String> = nfo.map(|n| n.directors.clone()).unwrap_or_default();
        sync_people_for_media(db, &cast, &directors, Some(movie_id), None, None).await?;
    }

    upload_extra_art(db, state, Some(movie_id), None, &artwork.extra_art).await?;

    Ok(VideoItemResult { video_item_id: movie_id, scraped })
}

async fn touch_video_item(
    db: &DatabaseConnection,
    movie_id: Uuid,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    video_items::Entity::update_many()
        .col_expr(video_items::Column::UpdatedAt, Expr::cust("NOW()"))
        .col_expr(video_items::Column::ScrapedAt, Expr::cust("NOW()"))
        .filter(video_items::Column::Id.eq(movie_id))
        .exec(db)
        .await?;
    Ok(())
}

async fn find_existing_video_item(
    db: &impl ConnectionTrait,
    app_id: Uuid,
    tmdb_id: Option<&str>,
    imdb_id: Option<&str>,
) -> Result<Option<Uuid>, Box<dyn std::error::Error + Send + Sync>> {
    if tmdb_id.is_none() && imdb_id.is_none() {
        return Ok(None);
    }
    let mut conditions = sea_orm::sea_query::Condition::any();
    if let Some(tid) = tmdb_id {
        conditions = conditions.add(video_items::Column::TmdbId.eq(tid));
    }
    if let Some(iid) = imdb_id {
        conditions = conditions.add(video_items::Column::ImdbId.eq(iid));
    }
    let existing = video_items::Entity::find()
        .filter(video_items::Column::VideoId.eq(app_id))
        .filter(conditions)
        .one(db)
        .await?;
    Ok(existing.map(|m| m.id))
}

/// Fallback dedup: match by title + year within the same library.
/// Only used when external IDs are unavailable (e.g. TMDB search failed).
async fn find_existing_video_item_by_title(
    db: &impl ConnectionTrait,
    app_id: Uuid,
    title: &str,
    year: Option<i32>,
) -> Result<Option<Uuid>, Box<dyn std::error::Error + Send + Sync>> {
    if title.is_empty() {
        return Ok(None);
    }
    let mut query = video_items::Entity::find()
        .filter(video_items::Column::VideoId.eq(app_id))
        .filter(video_items::Column::Title.eq(title));
    if let Some(y) = year {
        query = query.filter(video_items::Column::Year.eq(y));
    }
    let existing = query.one(db).await?;
    if let Some(ref m) = existing {
        info!(
            "[movie_scrape] Dedup by title+year: found existing movie '{}' ({})",
            title,
            m.id
        );
    }
    Ok(existing.map(|m| m.id))
}

/// Backfill external IDs onto an existing movie when a new file brings better metadata.
/// e.g. MKV was scraped with `tmdb_id` but BDMV matched by title — now copy `tmdb_id` over.
async fn backfill_video_item_ids(
    db: &impl ConnectionTrait,
    movie_id: Uuid,
    tmdb_detail: Option<&TmdbMediaDetail>,
    tmdb_id: Option<&str>,
    imdb_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let existing = video_items::Entity::find_by_id(movie_id).one(db).await?;
    let Some(existing) = existing else { return Ok(()) };

    let need_tmdb = existing.tmdb_id.is_none() && tmdb_id.is_some();
    let need_imdb = existing.imdb_id.is_none() && imdb_id.is_some();
    let need_overview = existing.overview.is_none() && tmdb_detail.and_then(|d| d.base.overview.as_ref()).is_some();

    if !need_tmdb && !need_imdb && !need_overview {
        return Ok(());
    }

    let mut update = video_items::Entity::update_many().filter(video_items::Column::Id.eq(movie_id));
    if need_tmdb {
        update = update.col_expr(video_items::Column::TmdbId, Expr::value(tmdb_id.unwrap()));
        info!("[movie_scrape] Backfilled tmdb_id={} onto movie {}", tmdb_id.unwrap(), movie_id);
    }
    if need_imdb {
        update = update.col_expr(video_items::Column::ImdbId, Expr::value(imdb_id.unwrap()));
    }
    if need_overview {
        let overview = tmdb_detail.unwrap().base.overview.clone().unwrap();
        update = update.col_expr(video_items::Column::Overview, Expr::value(overview));
    }
    update.exec(db).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_video_item_record(
    db: &impl ConnectionTrait,
    app_id: Uuid,
    is_adult: bool,
    should_use_tmdb: bool,
    tmdb_detail: Option<&TmdbMediaDetail>,
    nfo: Option<&NfoInfo>,
    parsed_title: &str,
    parsed_year: Option<i32>,
    tmdb_id_str: Option<&str>,
    imdb_id_str: Option<&str>,
    lib_type: LibType,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    let movie_id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();

    let title = tmdb_detail.map(|d| d.base.title.clone())
        .or_else(|| nfo.and_then(|n| n.title.clone()))
        .unwrap_or_else(|| parsed_title.to_string());

    let original_title = tmdb_detail.and_then(|d| d.base.original_title.clone())
        .or_else(|| nfo.and_then(|n| n.original_title.clone()));

    let year = tmdb_detail.and_then(|d| d.base.release_date.as_deref()
        .and_then(|r| r.get(..4)).and_then(|y| y.parse::<i32>().ok()))
        .or(parsed_year);

    let release_date = tmdb_detail.and_then(|d| d.base.release_date.as_deref()
        .and_then(|r| chrono::NaiveDate::parse_from_str(r, "%Y-%m-%d").ok()))
        .or_else(|| nfo.and_then(|n| n.release_date.as_deref()
            .and_then(|r| chrono::NaiveDate::parse_from_str(r, "%Y-%m-%d").ok())));

    let runtime = tmdb_detail.and_then(|d| d.runtime)
        .or_else(|| nfo.and_then(|n| n.runtime));

    let overview = tmdb_detail.and_then(|d| d.base.overview.clone())
        .or_else(|| nfo.and_then(|n| n.plot.clone()));
    let tagline = tmdb_detail.and_then(|d| d.tagline.clone())
        .or_else(|| nfo.and_then(|n| n.tagline.clone()));
    let tmdb_rating = if should_use_tmdb {
        tmdb_detail.and_then(|d| d.base.vote_average).or_else(|| nfo.and_then(|n| n.rating))
    } else {
        nfo.and_then(|n| n.rating)
    };
    let content_rating = nfo.and_then(|n| n.content_rating.clone());
    let countries = tmdb_detail.and_then(|d| d.origin_country.clone()).filter(|c| !c.is_empty());
    let scraped_at = if tmdb_detail.is_some()
        || nfo.is_some_and(crate::queue::handlers::nfo_parser::NfoInfo::is_sufficient)
        || lib_type == LibType::Custom
    { Some(now) } else { None };

    let mut metadata = serde_json::Map::new();
    if let Some(nfo) = nfo {
        if let Some(ref s) = nfo.studio { metadata.insert("studio".into(), json!(s)); }
        if let Some(ref c) = nfo.country { metadata.insert("country".into(), json!(c)); }
    }
    let metadata_json = if metadata.is_empty() { None } else { Some(serde_json::Value::Object(metadata)) };

    let model = video_items::ActiveModel {
        id: Set(movie_id),
        video_id: Set(app_id),
        title: Set(title.clone()),
        original_title: Set(original_title),
        sort_title: Set(None),
        year: Set(year),
        release_date: Set(release_date),
        runtime: Set(runtime),
        tmdb_rating: Set(tmdb_rating),
        imdb_rating: Set(None),
        douban_rating: Set(None),
        tmdb_id: Set(if should_use_tmdb { tmdb_id_str.map(std::string::ToString::to_string) } else { None }),
        imdb_id: Set(if should_use_tmdb { imdb_id_str.map(std::string::ToString::to_string) } else { None }),
        douban_id: Set(None),
        jav_number: Set(None),
        javbus_id: Set(None),
        javdb_id: Set(None),
        poster_path: Set(None),
        backdrop_path: Set(None),
        overview: Set(overview),
        tagline: Set(tagline),
        is_adult: Set(is_adult),
        is_favorite: Set(false),
        original_language: Set(tmdb_detail.and_then(|d| d.base.original_language.clone())),
        countries: Set(countries),
        spoken_languages: Set(None),
        content_rating: Set(content_rating),
        content_advisories: Set(None),
        locked_fields: Set(None),
        metadata: Set(metadata_json),
        scraped_at: Set(scraped_at),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
    };

    match video_items::Entity::insert(model).exec(db).await {
        Ok(_) => {
            info!("[movie_scrape] Created movie: {title} (tmdb={}, imdb={})",
                tmdb_id_str.unwrap_or("-"), imdb_id_str.unwrap_or("-"));
            Ok(movie_id)
        }
        Err(e) if is_unique_violation(&e) => {
            let existing = find_existing_video_item(db, app_id, tmdb_id_str, imdb_id_str).await?
                .or(find_existing_video_item_by_title(db, app_id, parsed_title, parsed_year).await?);
            if let Some(id) = existing {
                info!("[movie_scrape] Movie already exists (concurrent): {title}");
                Ok(id)
            } else {
                Err(e.into())
            }
        }
        Err(e) => Err(e.into()),
    }
}


