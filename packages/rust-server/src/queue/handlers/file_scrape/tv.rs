//! TV show, season, and episode creation logic aligned with TS file-scrape.ts.

use rust_client_api::metadata_providers::tmdb::{TmdbClient, TmdbMediaDetail};
use sea_orm::prelude::Expr;
use sea_orm::sea_query::Condition;
use sea_orm::*;
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::db::entities::{episodes, seasons, tv_shows};
use crate::db::repos::job_repo::JobRepo;
use crate::AppState;

use super::artwork::{upload_extra_art, upload_poster_and_backdrop, DiscoveredArtwork};
use crate::queue::handlers::common::{is_unique_violation, sync_genres, sync_genres_from_names, sync_people_for_media, CastMember};
use super::lib_type::LibType;
use super::nfo_parser::NfoInfo;
use super::tmdb;

pub struct TvResult {
    pub tv_show_id: Uuid,
    pub episode_id: Option<Uuid>,
}

/// Quick DB lookup before touching TMDB.
/// Returns `(show_id, tmdb_show_id)` when the show is already indexed.
/// Checks external IDs first (reliable), then falls back to title+year dedup
/// (mirrors movie's `find_existing_movie_by_title`).
async fn quick_find_existing(
    db: &DatabaseConnection,
    app_id: Uuid,
    nfo: &Option<NfoInfo>,
    title: &str,
    year: Option<i32>,
) -> Result<Option<(Uuid, Option<i64>)>, Box<dyn std::error::Error + Send + Sync>> {
    // 1. External IDs from NFO (most reliable — no false positives)
    let nfo_tmdb_id = nfo.as_ref().and_then(|n| n.tmdb_id.as_deref());
    let nfo_imdb_id = nfo.as_ref().and_then(|n| n.imdb_id.as_deref());
    if nfo_tmdb_id.is_some() || nfo_imdb_id.is_some() {
        let mut cond = Condition::any();
        if let Some(tid) = nfo_tmdb_id { cond = cond.add(tv_shows::Column::TmdbId.eq(tid)); }
        if let Some(iid) = nfo_imdb_id { cond = cond.add(tv_shows::Column::ImdbId.eq(iid)); }
        if let Some(show) = tv_shows::Entity::find()
            .filter(tv_shows::Column::AppId.eq(app_id))
            .filter(cond)
            .one(db).await?
        {
            let tmdb_id = show.tmdb_id.as_deref().and_then(|s| s.parse::<i64>().ok());
            return Ok(Some((show.id, tmdb_id)));
        }
    }

    // 2. Title+year fallback — catches shows already imported without NFO external IDs
    //    (the TMDB call for the first episode stores tmdb_id on the record, so
    //    subsequent episodes with a matching title skip the search entirely)
    if !title.is_empty() {
        let mut q = tv_shows::Entity::find()
            .filter(tv_shows::Column::AppId.eq(app_id))
            .filter(tv_shows::Column::Title.eq(title));
        if let Some(y) = year {
            q = q.filter(tv_shows::Column::Year.eq(y));
        }
        if let Some(show) = q.one(db).await? {
            let tmdb_id = show.tmdb_id.as_deref().and_then(|s| s.parse::<i64>().ok());
            info!("[file_scrape] TV dedup by title+year: '{title}' → {}", show.id);
            return Ok(Some((show.id, tmdb_id)));
        }
    }

    Ok(None)
}

/// Single entry point called from mod.rs.
///
/// Fast path: if the show is already in the DB (looked up by NFO external IDs
/// or title+year), we skip the TMDB search+detail entirely — same as movies.
/// The TMDB client is still built so new seasons can be fetched on demand.
#[allow(clippy::too_many_arguments)]
pub async fn scrape(
    db: &DatabaseConnection,
    state: &std::sync::Arc<AppState>,
    app_id: Uuid,
    lib_type: LibType,
    nfo: &Option<NfoInfo>,
    title: &str,
    year: Option<i32>,
    season: Option<i32>,
    episode: Option<i32>,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb: &Option<String>,
    nfo_backdrop_tmdb: &Option<String>,
) -> Result<TvResult, Box<dyn std::error::Error + Send + Sync>> {
    let _ = lib_type; // currently all TV uses TMDB

    let api_key = tmdb::get_api_key(db).await?;

    // Pre-check: is this show already in the DB?
    // If yes, skip the expensive TMDB search+detail (2 HTTP calls → 0).
    let pre_existing = quick_find_existing(db, app_id, nfo, title, year).await?;

    let mut tmdb_detail: Option<TmdbMediaDetail> = None;
    let mut tmdb_client: Option<TmdbClient> = None;

    if let Some(ref key) = api_key {
        let client = tmdb::build_client(state, key);
        if pre_existing.is_none() {
            // New show — run full TMDB scrape (search + detail)
            tmdb_detail = tmdb::scrape_tv(
                &client, nfo, title, year, artwork, nfo_poster_tmdb, nfo_backdrop_tmdb,
            )
            .await;
        }
        // Keep client so new seasons can call get_tv_season_detail when needed
        tmdb_client = Some(client);
    }

    find_or_create_tv(
        db, state, tmdb_client.as_ref(),
        app_id, pre_existing, tmdb_detail.as_ref(), nfo.as_ref(),
        title, year, season, episode,
        artwork, nfo_poster_tmdb.as_deref(), nfo_backdrop_tmdb.as_deref(),
    )
    .await
}

/// Find or create a TV show, then create season/episode if we have numbers.
///
/// `pre_existing` — result from `quick_find_existing`: `(show_id, tmdb_show_id)`.
/// When provided the internal DB lookup is skipped (avoids a redundant query)
/// and the stored `tmdb_show_id` is used for season fetching even when
/// `tmdb_detail` is `None` (i.e. TMDB scrape was skipped for an existing show).
#[allow(clippy::too_many_arguments)]
pub async fn find_or_create_tv(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    tmdb: Option<&TmdbClient>,
    app_id: Uuid,
    pre_existing: Option<(Uuid, Option<i64>)>,
    tmdb_detail: Option<&TmdbMediaDetail>,
    nfo: Option<&NfoInfo>,
    parsed_title: &str,
    parsed_year: Option<i32>,
    parsed_season: Option<i32>,
    parsed_episode: Option<i32>,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb_path: Option<&str>,
    nfo_backdrop_tmdb_path: Option<&str>,
) -> Result<TvResult, Box<dyn std::error::Error + Send + Sync>> {
    let tmdb_id_str = tmdb_detail
        .map(|d| d.base.id.to_string())
        .or_else(|| nfo.and_then(|n| n.tmdb_id.clone()));
    let imdb_id_str = tmdb_detail
        .and_then(|d| d.imdb_id.clone())
        .or_else(|| nfo.and_then(|n| n.imdb_id.clone()));

    // Use pre_existing from caller when available (avoids a redundant DB query).
    // Fall back to a DB lookup only when scrape() couldn't pre-check (shouldn't
    // normally happen, but keeps the function usable standalone).
    let resolved_existing = if let Some((id, _)) = pre_existing {
        Some(id)
    } else {
        find_existing_tv_show(db, app_id, tmdb_id_str.as_deref(), imdb_id_str.as_deref()).await?
    };

    let (tv_show_id, is_new) = if let Some(existing_id) = resolved_existing {
        tv_shows::Entity::update_many()
            .col_expr(tv_shows::Column::UpdatedAt, Expr::cust("NOW()"))
            .col_expr(tv_shows::Column::ScrapedAt, Expr::cust("NOW()"))
            .filter(tv_shows::Column::Id.eq(existing_id))
            .exec(db).await?;
        (existing_id, false)
    } else {
        let id = create_tv_show_record(
            db, app_id, tmdb_detail, nfo, parsed_title, parsed_year,
            tmdb_id_str.as_deref(), imdb_id_str.as_deref(),
        ).await?;
        (id, true)
    };

    if is_new {
        // Artwork
        let (poster_path, backdrop_path) = upload_poster_and_backdrop(
            db, state, "tvShow", tv_show_id, artwork,
            nfo_poster_tmdb_path, nfo_backdrop_tmdb_path,
            tmdb_detail.and_then(|d| d.base.poster_path.as_deref()),
            tmdb_detail.and_then(|d| d.base.backdrop_path.as_deref()),
        ).await?;

        if poster_path.is_some() || backdrop_path.is_some() {
            let mut update = tv_shows::Entity::update_many().filter(tv_shows::Column::Id.eq(tv_show_id));
            if let Some(pp) = &poster_path {
                update = update.col_expr(tv_shows::Column::PosterPath, Expr::value(pp.as_str()));
            }
            if let Some(bp) = &backdrop_path {
                update = update.col_expr(tv_shows::Column::BackdropPath, Expr::value(bp.as_str()));
            }
            update.exec(db).await?;
        }

        // Genres (TMDB preferred, NFO fallback)
        if let Some(detail) = tmdb_detail {
            if let Some(genres) = &detail.genres {
                sync_genres(db, genres, None, Some(tv_show_id)).await?;
            }
        } else if let Some(nfo) = nfo
            && !nfo.genres.is_empty() {
                sync_genres_from_names(db, &nfo.genres, None, Some(tv_show_id)).await?;
            }
        // Cast + directors: unified sync (aligned with TS syncPeopleForMedia)
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
            sync_people_for_media(db, &cast, &directors, None, Some(tv_show_id)).await?;
        }

        upload_extra_art(db, state, None, Some(tv_show_id), &artwork.extra_art).await?;
    }

    // Season + Episode
    let season_num = nfo.and_then(|n| n.season).or(parsed_season);
    let episode_num = nfo.and_then(|n| n.episode).or(parsed_episode);

    let episode_id = if let (Some(sn), Some(en)) = (season_num, episode_num) {
        // Prefer tmdb_id from the freshly-fetched detail; fall back to the
        // id stored in DB (via pre_existing) or the NFO — so new seasons can
        // still call get_tv_season_detail even when the show scrape was skipped.
        let tmdb_show_id = tmdb_detail
            .map(|d| d.base.id)
            .or_else(|| pre_existing.and_then(|(_, sid)| sid))
            .or_else(|| tmdb_id_str.as_deref().and_then(|s| s.parse::<i64>().ok()));
        match create_season_and_episode(db, tmdb, tmdb_show_id, tv_show_id, sn, en, nfo).await {
            Ok(eid) => Some(eid),
            Err(e) => { warn!("[file_scrape] Failed to create season/episode: {e}"); None }
        }
    } else { None };

    Ok(TvResult { tv_show_id, episode_id })
}

async fn find_existing_tv_show(
    db: &DatabaseConnection, app_id: Uuid,
    tmdb_id: Option<&str>, imdb_id: Option<&str>,
) -> Result<Option<Uuid>, Box<dyn std::error::Error + Send + Sync>> {
    if tmdb_id.is_none() && imdb_id.is_none() { return Ok(None); }
    let mut conditions = sea_orm::sea_query::Condition::any();
    if let Some(tid) = tmdb_id { conditions = conditions.add(tv_shows::Column::TmdbId.eq(tid)); }
    if let Some(iid) = imdb_id { conditions = conditions.add(tv_shows::Column::ImdbId.eq(iid)); }
    let existing = tv_shows::Entity::find()
        .filter(tv_shows::Column::AppId.eq(app_id))
        .filter(conditions)
        .one(db).await?;
    Ok(existing.map(|s| s.id))
}

#[allow(clippy::too_many_arguments)]
async fn create_tv_show_record(
    db: &DatabaseConnection, app_id: Uuid,
    tmdb_detail: Option<&TmdbMediaDetail>, nfo: Option<&NfoInfo>,
    parsed_title: &str, parsed_year: Option<i32>,
    tmdb_id_str: Option<&str>, imdb_id_str: Option<&str>,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    let show_id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();

    let title = tmdb_detail.map(|d| d.base.title.clone())
        .or_else(|| nfo.and_then(|n| n.title.clone()))
        .unwrap_or_else(|| parsed_title.to_string());
    let original_title = tmdb_detail.and_then(|d| d.base.original_title.clone())
        .or_else(|| nfo.and_then(|n| n.original_title.clone()));
    let year = tmdb_detail.and_then(|d| d.base.release_date.as_deref()
        .and_then(|r| r.get(..4)).and_then(|y| y.parse::<i32>().ok()))
        .or(parsed_year);
    let first_air_date = tmdb_detail.and_then(|d| d.base.release_date.as_deref()
        .and_then(|r| chrono::NaiveDate::parse_from_str(r, "%Y-%m-%d").ok()));
    let overview = tmdb_detail.and_then(|d| d.base.overview.clone())
        .or_else(|| nfo.and_then(|n| n.plot.clone()));
    let tmdb_rating = tmdb_detail.and_then(|d| d.base.vote_average)
        .or_else(|| nfo.and_then(|n| n.rating));
    let content_rating = nfo.and_then(|n| n.content_rating.clone());
    let countries = tmdb_detail.and_then(|d| d.origin_country.clone()).filter(|c| !c.is_empty());
    let scraped_at = if tmdb_detail.is_some() || nfo.is_some_and(super::super::nfo_parser::NfoInfo::is_sufficient) { Some(now) } else { None };

    let mut metadata_map = serde_json::Map::new();
    if let Some(nfo) = nfo {
        if let Some(ref s) = nfo.studio { metadata_map.insert("studio".into(), json!(s)); }
        if let Some(ref c) = nfo.country { metadata_map.insert("country".into(), json!(c)); }
    }
    let metadata_json = if metadata_map.is_empty() { None } else { Some(serde_json::Value::Object(metadata_map)) };

    let model = tv_shows::ActiveModel {
        id: Set(show_id), app_id: Set(app_id),
        title: Set(title.clone()), original_title: Set(original_title), sort_title: Set(None),
        year: Set(year), first_air_date: Set(first_air_date), last_air_date: Set(None),
        status: Set(tmdb_detail.and_then(|d| d.status.clone())),
        tmdb_rating: Set(tmdb_rating), imdb_rating: Set(None), douban_rating: Set(None),
        tmdb_id: Set(tmdb_id_str.map(std::string::ToString::to_string)),
        imdb_id: Set(imdb_id_str.map(std::string::ToString::to_string)),
        tvdb_id: Set(None), douban_id: Set(None), bangumi_id: Set(None),
        poster_path: Set(None), backdrop_path: Set(None),
        overview: Set(overview), is_adult: Set(false), is_favorite: Set(false),
        original_language: Set(tmdb_detail.and_then(|d| d.base.original_language.clone())),
        countries: Set(countries), content_rating: Set(content_rating),
        content_advisories: Set(None), locked_fields: Set(None),
        metadata: Set(metadata_json), scraped_at: Set(scraped_at),
        created_at: Set(Some(now)), updated_at: Set(Some(now)),
    };

    match tv_shows::Entity::insert(model).exec(db).await {
        Ok(_) => {
            info!("[file_scrape] Created TV show: {title} (tmdb={}, imdb={})",
                tmdb_id_str.unwrap_or("-"), imdb_id_str.unwrap_or("-"));
            Ok(show_id)
        }
        Err(e) if is_unique_violation(&e) => {
            let existing = find_existing_tv_show(db, app_id, tmdb_id_str, imdb_id_str).await?;
            if let Some(id) = existing {
                info!("[file_scrape] TV show already exists (concurrent): {title}");
                Ok(id)
            } else { Err(e.into()) }
        }
        Err(e) => Err(e.into()),
    }
}

// ── Season / Episode ──

async fn create_season_and_episode(
    db: &DatabaseConnection, tmdb: Option<&TmdbClient>, tmdb_show_id: Option<i64>,
    show_id: Uuid, season_number: i32, episode_number: i32, nfo: Option<&NfoInfo>,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    // Check season existence BEFORE any TMDB call.
    // For 24 episodes of the same season this saves 23 get_tv_season_detail requests.
    let season_exists = seasons::Entity::find()
        .filter(seasons::Column::TvShowId.eq(show_id))
        .filter(seasons::Column::SeasonNumber.eq(season_number))
        .one(db).await?.is_some();

    let season_detail = if season_exists { None } else if let (Some(tmdb), Some(sid)) = (tmdb, tmdb_show_id) {
        tmdb.get_tv_season_detail(sid, season_number).await.ok()
    } else { None };

    let season_id = upsert_season(db, show_id, season_number, season_detail.as_ref()).await?;
    let tmdb_episode = season_detail.as_ref()
        .and_then(|sd| sd.episodes.as_ref())
        .and_then(|eps| eps.iter().find(|e| e.episode_number == episode_number));
    upsert_episode(db, show_id, season_id, episode_number, tmdb_episode, nfo).await
}

async fn upsert_season(
    db: &DatabaseConnection, show_id: Uuid, season_number: i32,
    tmdb_detail: Option<&rust_client_api::metadata_providers::tmdb::TmdbSeasonDetail>,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    let existing = seasons::Entity::find()
        .filter(seasons::Column::TvShowId.eq(show_id))
        .filter(seasons::Column::SeasonNumber.eq(season_number))
        .one(db).await?;
    if let Some(existing) = existing { return Ok(existing.id); }

    let season_id = Uuid::new_v4();
    let (title, overview, air_date, poster_path, episode_count) = if let Some(sd) = tmdb_detail {
        let air = sd.episodes.as_ref()
            .and_then(|eps| eps.first())
            .and_then(|e| e.air_date.as_deref())
            .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
        let ep_count = sd.episodes.as_ref().map(|eps| eps.len() as i32);
        (Some(sd.name.clone()), sd.overview.clone(), air, sd.poster_path.clone(), ep_count)
    } else {
        (Some(format!("Season {season_number}")), None, None, None, None)
    };

    let model = seasons::ActiveModel {
        id: Set(season_id), tv_show_id: Set(show_id), season_number: Set(season_number),
        title: Set(title), overview: Set(overview), air_date: Set(air_date),
        poster_path: Set(None), episode_count: Set(episode_count),
    };

    match seasons::Entity::insert(model).exec(db).await {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            let existing = seasons::Entity::find()
                .filter(seasons::Column::TvShowId.eq(show_id))
                .filter(seasons::Column::SeasonNumber.eq(season_number))
                .one(db).await?;
            if let Some(existing) = existing { return Ok(existing.id); }
            return Err(e.into());
        }
        Err(e) => return Err(e.into()),
    }

    if let Some(poster) = poster_path {
        let storage_key = format!("tmdb-images/seasons/{season_id}/poster.jpg");
        let url = format!("https://image.tmdb.org/t/p/w500{poster}");
        let _ = JobRepo::create_job(db, "image_upload",
            json!({ "plexUrl": url, "storageKey": storage_key,
                "entity": "season", "entityId": season_id.to_string(), "field": "posterPath" }), None).await;
    }
    Ok(season_id)
}

async fn upsert_episode(
    db: &DatabaseConnection, show_id: Uuid, season_id: Uuid, episode_number: i32,
    tmdb_ep: Option<&rust_client_api::metadata_providers::tmdb::TmdbEpisode>,
    nfo: Option<&NfoInfo>,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    let existing = episodes::Entity::find()
        .filter(episodes::Column::SeasonId.eq(season_id))
        .filter(episodes::Column::EpisodeNumber.eq(episode_number))
        .one(db).await?;
    if let Some(existing) = existing { return Ok(existing.id); }

    let episode_id = Uuid::new_v4();
    let (title, overview, air_date, runtime, still_path, tmdb_rating, tmdb_id) =
        if let Some(ep) = tmdb_ep {
            let air = ep.air_date.as_deref()
                .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
            (Some(ep.name.clone()), ep.overview.clone(), air, None::<i32>,
             ep.still_path.clone(), ep.vote_average, Some(ep.id.to_string()))
        } else {
            (nfo.and_then(|n| n.title.clone()).or(Some(format!("Episode {episode_number}"))),
             nfo.and_then(|n| n.plot.clone()), None, None, None, None, None)
        };

    let model = episodes::ActiveModel {
        id: Set(episode_id), tv_show_id: Set(show_id), season_id: Set(season_id),
        episode_number: Set(episode_number), title: Set(title), overview: Set(overview),
        air_date: Set(air_date), runtime: Set(runtime), still_path: Set(None),
        tmdb_rating: Set(tmdb_rating), tmdb_id: Set(tmdb_id),
    };

    match episodes::Entity::insert(model).exec(db).await {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            let existing = episodes::Entity::find()
                .filter(episodes::Column::SeasonId.eq(season_id))
                .filter(episodes::Column::EpisodeNumber.eq(episode_number))
                .one(db).await?;
            if let Some(existing) = existing { return Ok(existing.id); }
            return Err(e.into());
        }
        Err(e) => return Err(e.into()),
    }

    if let Some(still) = still_path {
        let storage_key = format!("tmdb-images/episodes/{episode_id}/still.jpg");
        let url = format!("https://image.tmdb.org/t/p/w500{still}");
        let _ = JobRepo::create_job(db, "image_upload",
            json!({ "plexUrl": url, "storageKey": storage_key,
                "entity": "episode", "entityId": episode_id.to_string(), "field": "stillPath" }), None).await;
    }
    Ok(episode_id)
}
