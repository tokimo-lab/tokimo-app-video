//! Movie creation and lookup logic aligned with TS file-scrape.ts.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rust_client_api::metadata_providers::tmdb::TmdbMediaDetail;
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde_json::json;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use crate::db::entities::movies;
use crate::AppState;

use super::artwork::{upload_extra_art, upload_poster_and_backdrop, DiscoveredArtwork};
use crate::queue::handlers::common::{is_unique_violation, sync_genres, sync_genres_from_names, sync_people_for_media, CastMember};
use super::lib_type::LibType;
use super::nfo_parser::NfoInfo;
use super::tmdb;

pub struct MovieResult {
    pub movie_id: Uuid,
    pub scraped: bool,
}

/// Single entry point called from mod.rs.
/// Fetches TMDB detail if needed, then finds or creates the movie record.
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
) -> Result<MovieResult, Box<dyn std::error::Error + Send + Sync>> {
    let mut tmdb_detail: Option<TmdbMediaDetail> = None;

    if lib_type.uses_tmdb()
        && let Some(api_key) = tmdb::get_api_key(db).await? {
            let client = tmdb::build_client(state, &api_key);
            tmdb_detail = tmdb::scrape_movie(
                &client, nfo, title, year, artwork, nfo_poster_tmdb, nfo_backdrop_tmdb,
            )
            .await;
        }

    let scraped =
        tmdb_detail.is_some() || nfo.as_ref().is_some_and(super::super::nfo_parser::NfoInfo::is_sufficient);

    let result = find_or_create_movie(
        db, state, app_id, lib_type,
        tmdb_detail.as_ref(), nfo.as_ref(),
        parsed_title, parsed_year,
        artwork, nfo_poster_tmdb.as_deref(), nfo_backdrop_tmdb.as_deref(),
    )
    .await?;

    Ok(MovieResult { movie_id: result.movie_id, scraped })
}

/// Compute a stable i64 advisory lock key from (`app_id`, title, year).
fn movie_lock_key(app_id: Uuid, title: &str, year: Option<i32>) -> i64 {
    let mut h = DefaultHasher::new();
    app_id.hash(&mut h);
    title.to_lowercase().hash(&mut h);
    year.hash(&mut h);
    h.finish() as i64
}

/// Find or create a movie record, fully aligned with TS logic.
/// Uses a `PostgreSQL` advisory lock keyed on (`app_id`, title, year) to prevent
/// concurrent workers from creating duplicate movie records when `tmdb_id` is NULL.
#[allow(clippy::too_many_arguments)]
pub async fn find_or_create_movie(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    app_id: Uuid,
    lib_type: LibType,
    tmdb_detail: Option<&TmdbMediaDetail>,
    nfo: Option<&NfoInfo>,
    parsed_title: &str,
    parsed_year: Option<i32>,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb_path: Option<&str>,
    nfo_backdrop_tmdb_path: Option<&str>,
) -> Result<MovieResult, Box<dyn std::error::Error + Send + Sync>> {
    let should_use_tmdb = lib_type.uses_tmdb();
    let is_adult = lib_type.is_adult();

    let tmdb_id_str = tmdb_detail
        .map(|d| d.base.id.to_string())
        .or_else(|| nfo.and_then(|n| n.tmdb_id.clone()));
    let imdb_id_str = tmdb_detail
        .and_then(|d| d.imdb_id.clone())
        .or_else(|| nfo.and_then(|n| n.imdb_id.clone()));

    // Acquire advisory lock to serialize concurrent movie creation for the same title+year.
    // This prevents the race where multiple workers all find no existing movie and all insert.
    let lock_key = movie_lock_key(app_id, parsed_title, parsed_year);
    let txn = db.begin().await?;
    txn.execute_unprepared(&format!("SELECT pg_advisory_xact_lock({lock_key})"))
        .await?;

    // Check existing by external IDs first, then fall back to title+year (same library)
    let existing = find_existing_movie(&txn, app_id, tmdb_id_str.as_deref(), imdb_id_str.as_deref()).await?
        .or(find_existing_movie_by_title(&txn, app_id, parsed_title, parsed_year).await?);

    let movie_id = if let Some(existing_id) = existing {
        // If this file brought new external IDs, backfill them onto the existing record
        backfill_external_ids(&txn, existing_id, tmdb_detail, tmdb_id_str.as_deref(), imdb_id_str.as_deref()).await?;
        movies::Entity::update_many()
            .col_expr(movies::Column::UpdatedAt, Expr::cust("NOW()"))
            .col_expr(movies::Column::ScrapedAt, Expr::cust("NOW()"))
            .filter(movies::Column::Id.eq(existing_id))
            .exec(&txn)
            .await?;
        txn.commit().await?;
        existing_id
    } else {
        let id = create_movie_record(
            &txn, app_id, is_adult, should_use_tmdb, tmdb_detail, nfo,
            parsed_title, parsed_year, tmdb_id_str.as_deref(), imdb_id_str.as_deref(), lib_type,
        )
        .await?;
        txn.commit().await?;
        id
    };

    // Upload artwork
    let (poster_path, backdrop_path) = upload_poster_and_backdrop(
        db, state, "movie", movie_id, artwork,
        nfo_poster_tmdb_path, nfo_backdrop_tmdb_path,
        tmdb_detail.and_then(|d| d.base.poster_path.as_deref()),
        tmdb_detail.and_then(|d| d.base.backdrop_path.as_deref()),
    )
    .await?;

    if poster_path.is_some() || backdrop_path.is_some() {
        let mut update = movies::Entity::update_many().filter(movies::Column::Id.eq(movie_id));
        if let Some(pp) = &poster_path {
            update = update.col_expr(movies::Column::PosterPath, Expr::value(pp.as_str()));
        }
        if let Some(bp) = &backdrop_path {
            update = update.col_expr(movies::Column::BackdropPath, Expr::value(bp.as_str()));
        }
        update.exec(db).await?;
    }

    // Sync genres (TMDB preferred, NFO fallback)
    if let Some(detail) = tmdb_detail {
        if let Some(genres) = &detail.genres {
            sync_genres(db, genres, Some(movie_id), None).await?;
        }
    } else if let Some(nfo) = nfo
        && !nfo.genres.is_empty() {
            sync_genres_from_names(db, &nfo.genres, Some(movie_id), None).await?;
        }

    // Sync cast/people: unified approach (TMDB cast preferred, NFO actors fallback + NFO directors)
    // Aligned with TS: single syncPeopleForMedia call with aggregated cast + directors
    {
        let cast: Vec<CastMember> = if let Some(detail) = tmdb_detail {
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
        sync_people_for_media(db, &cast, &directors, Some(movie_id), None).await?;
    }

    // Upload extra art
    upload_extra_art(db, state, Some(movie_id), None, &artwork.extra_art).await?;

    Ok(MovieResult { movie_id, scraped: false })
}

async fn find_existing_movie(
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
        conditions = conditions.add(movies::Column::TmdbId.eq(tid));
    }
    if let Some(iid) = imdb_id {
        conditions = conditions.add(movies::Column::ImdbId.eq(iid));
    }
    let existing = movies::Entity::find()
        .filter(movies::Column::AppId.eq(app_id))
        .filter(conditions)
        .one(db)
        .await?;
    Ok(existing.map(|m| m.id))
}

/// Fallback dedup: match by title + year within the same library.
/// Only used when external IDs are unavailable (e.g. TMDB search failed).
async fn find_existing_movie_by_title(
    db: &impl ConnectionTrait,
    app_id: Uuid,
    title: &str,
    year: Option<i32>,
) -> Result<Option<Uuid>, Box<dyn std::error::Error + Send + Sync>> {
    if title.is_empty() {
        return Ok(None);
    }
    let mut query = movies::Entity::find()
        .filter(movies::Column::AppId.eq(app_id))
        .filter(movies::Column::Title.eq(title));
    if let Some(y) = year {
        query = query.filter(movies::Column::Year.eq(y));
    }
    let existing = query.one(db).await?;
    if let Some(ref m) = existing {
        info!(
            "[file_scrape] Dedup by title+year: found existing movie '{}' ({})",
            title,
            m.id
        );
    }
    Ok(existing.map(|m| m.id))
}

/// Backfill external IDs onto an existing movie when a new file brings better metadata.
/// e.g. MKV was scraped with `tmdb_id` but BDMV matched by title — now copy `tmdb_id` over.
async fn backfill_external_ids(
    db: &impl ConnectionTrait,
    movie_id: Uuid,
    tmdb_detail: Option<&TmdbMediaDetail>,
    tmdb_id: Option<&str>,
    imdb_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let existing = movies::Entity::find_by_id(movie_id).one(db).await?;
    let Some(existing) = existing else { return Ok(()) };

    let need_tmdb = existing.tmdb_id.is_none() && tmdb_id.is_some();
    let need_imdb = existing.imdb_id.is_none() && imdb_id.is_some();
    let need_overview = existing.overview.is_none() && tmdb_detail.and_then(|d| d.base.overview.as_ref()).is_some();

    if !need_tmdb && !need_imdb && !need_overview {
        return Ok(());
    }

    let mut update = movies::Entity::update_many().filter(movies::Column::Id.eq(movie_id));
    if need_tmdb {
        update = update.col_expr(movies::Column::TmdbId, Expr::value(tmdb_id.unwrap()));
        info!("[file_scrape] Backfilled tmdb_id={} onto movie {}", tmdb_id.unwrap(), movie_id);
    }
    if need_imdb {
        update = update.col_expr(movies::Column::ImdbId, Expr::value(imdb_id.unwrap()));
    }
    if need_overview {
        let overview = tmdb_detail.unwrap().base.overview.clone().unwrap();
        update = update.col_expr(movies::Column::Overview, Expr::value(overview));
    }
    update.exec(db).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_movie_record(
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
        || nfo.is_some_and(super::super::nfo_parser::NfoInfo::is_sufficient)
        || lib_type == LibType::Custom
    { Some(now) } else { None };

    let mut metadata = serde_json::Map::new();
    if let Some(nfo) = nfo {
        if let Some(ref s) = nfo.studio { metadata.insert("studio".into(), json!(s)); }
        if let Some(ref c) = nfo.country { metadata.insert("country".into(), json!(c)); }
    }
    let metadata_json = if metadata.is_empty() { None } else { Some(serde_json::Value::Object(metadata)) };

    let model = movies::ActiveModel {
        id: Set(movie_id),
        app_id: Set(app_id),
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

    match movies::Entity::insert(model).exec(db).await {
        Ok(_) => {
            info!("[file_scrape] Created movie: {title} (tmdb={}, imdb={})",
                tmdb_id_str.unwrap_or("-"), imdb_id_str.unwrap_or("-"));
            Ok(movie_id)
        }
        Err(e) if is_unique_violation(&e) => {
            let existing = find_existing_movie(db, app_id, tmdb_id_str, imdb_id_str).await?
                .or(find_existing_movie_by_title(db, app_id, parsed_title, parsed_year).await?);
            if let Some(id) = existing {
                info!("[file_scrape] Movie already exists (concurrent): {title}");
                Ok(id)
            } else {
                Err(e.into())
            }
        }
        Err(e) => Err(e.into()),
    }
}


