//! Movie creation and lookup logic aligned with TS file-scrape.ts.

use rust_client_api::metadata_providers::tmdb::TmdbMediaDetail;
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde_json::json;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use crate::db::entities::{download_records, movies};
use crate::AppState;

use super::artwork::{upload_extra_art, upload_poster_and_backdrop, DiscoveredArtwork};
use super::common::{is_unique_violation, sync_genres, sync_genres_from_names, sync_people_for_media, CastMember};
use super::nfo_parser::NfoInfo;

pub struct MovieResult {
    pub movie_id: Uuid,
}

/// Find or create a movie record, fully aligned with TS logic.
#[allow(clippy::too_many_arguments)]
pub async fn find_or_create_movie(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    library_id: Uuid,
    lib_type: &str,
    tmdb_detail: Option<&TmdbMediaDetail>,
    nfo: Option<&NfoInfo>,
    online_record: Option<&download_records::Model>,
    parsed_title: &str,
    parsed_year: Option<i32>,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb_path: Option<&str>,
    nfo_backdrop_tmdb_path: Option<&str>,
) -> Result<MovieResult, Box<dyn std::error::Error + Send + Sync>> {
    let should_use_tmdb = !matches!(lib_type, "custom" | "online_video");
    let is_adult = lib_type == "adult";

    let tmdb_id_str = tmdb_detail
        .map(|d| d.base.id.to_string())
        .or_else(|| nfo.and_then(|n| n.tmdb_id.clone()));
    let imdb_id_str = tmdb_detail
        .and_then(|d| d.imdb_id.clone())
        .or_else(|| nfo.and_then(|n| n.imdb_id.clone()));

    // Check existing by external IDs (same library)
    let existing = find_existing_movie(db, library_id, tmdb_id_str.as_deref(), imdb_id_str.as_deref()).await?;

    let movie_id = if let Some(existing_id) = existing {
        movies::Entity::update_many()
            .col_expr(movies::Column::UpdatedAt, Expr::cust("NOW()"))
            .col_expr(movies::Column::ScrapedAt, Expr::cust("NOW()"))
            .filter(movies::Column::Id.eq(existing_id))
            .exec(db)
            .await?;
        existing_id
    } else {
        create_movie_record(
            db, library_id, is_adult, should_use_tmdb, tmdb_detail, nfo, online_record,
            parsed_title, parsed_year, tmdb_id_str.as_deref(), imdb_id_str.as_deref(), lib_type,
        )
        .await?
    };

    // Upload artwork
    let (mut poster_path, backdrop_path) = upload_poster_and_backdrop(
        db, state, "movie", movie_id, artwork,
        nfo_poster_tmdb_path, nfo_backdrop_tmdb_path,
        tmdb_detail.and_then(|d| d.base.poster_path.as_deref()),
        tmdb_detail.and_then(|d| d.base.backdrop_path.as_deref()),
    )
    .await?;

    // Online video thumbnail fallback: download remote thumbnail as poster
    if poster_path.is_none() {
        if let Some(thumb_url) = online_record.and_then(|r| r.thumbnail_url.as_deref()) {
            if !thumb_url.is_empty() {
                match download_thumbnail(state, thumb_url, "movies", &movie_id.to_string()).await {
                    Ok(sp) => { poster_path = Some(sp); }
                    Err(e) => { tracing::warn!("[file_scrape] thumbnail download failed: {e}"); }
                }
            }
        }
    }

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
    } else if let Some(nfo) = nfo {
        if !nfo.genres.is_empty() {
            sync_genres_from_names(db, &nfo.genres, Some(movie_id), None).await?;
        }
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
            }).collect()
        } else {
            vec![]
        };
        let directors: Vec<String> = nfo.map(|n| n.directors.clone()).unwrap_or_default();
        sync_people_for_media(db, &cast, &directors, Some(movie_id), None).await?;
    }

    // Upload extra art
    upload_extra_art(db, state, Some(movie_id), None, &artwork.extra_art).await?;

    Ok(MovieResult { movie_id })
}

async fn find_existing_movie(
    db: &DatabaseConnection,
    library_id: Uuid,
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
        .filter(movies::Column::LibraryId.eq(library_id))
        .filter(conditions)
        .one(db)
        .await?;
    Ok(existing.map(|m| m.id))
}

#[allow(clippy::too_many_arguments)]
async fn create_movie_record(
    db: &DatabaseConnection,
    library_id: Uuid,
    is_adult: bool,
    should_use_tmdb: bool,
    tmdb_detail: Option<&TmdbMediaDetail>,
    nfo: Option<&NfoInfo>,
    online_record: Option<&download_records::Model>,
    parsed_title: &str,
    parsed_year: Option<i32>,
    tmdb_id_str: Option<&str>,
    imdb_id_str: Option<&str>,
    lib_type: &str,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    let movie_id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();

    let title = tmdb_detail.map(|d| d.base.title.clone())
        .or_else(|| nfo.and_then(|n| n.title.clone()))
        .or_else(|| online_record.and_then(|r| r.media_title.clone()))
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
        .or_else(|| nfo.and_then(|n| n.runtime))
        .or_else(|| online_record.and_then(|r| r.duration_seconds)
            .map(|s| (s as f64 / 60.0).round() as i32));

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
        || nfo.is_some_and(|n| n.is_sufficient())
        || matches!(lib_type, "custom" | "online_video")
    { Some(now) } else { None };

    let mut metadata = serde_json::Map::new();
    if let Some(nfo) = nfo {
        if let Some(ref s) = nfo.studio { metadata.insert("studio".into(), json!(s)); }
        if let Some(ref c) = nfo.country { metadata.insert("country".into(), json!(c)); }
    }
    if let Some(or) = online_record {
        if let Some(ref u) = or.uploader { metadata.insert("uploader".into(), json!(u)); }
        if let Some(ref s) = or.source_site { metadata.insert("sourceSite".into(), json!(s)); }
        if let Some(ref e) = or.external_id { metadata.insert("externalId".into(), json!(e)); }
    }
    let metadata_json = if metadata.is_empty() { None } else { Some(serde_json::Value::Object(metadata)) };

    let model = movies::ActiveModel {
        id: Set(movie_id),
        library_id: Set(library_id),
        title: Set(title.clone()),
        original_title: Set(original_title),
        sort_title: Set(None),
        year: Set(year),
        release_date: Set(release_date),
        runtime: Set(runtime),
        tmdb_rating: Set(tmdb_rating),
        imdb_rating: Set(None),
        douban_rating: Set(None),
        tmdb_id: Set(if should_use_tmdb { tmdb_id_str.map(|s| s.to_string()) } else { None }),
        imdb_id: Set(if should_use_tmdb { imdb_id_str.map(|s| s.to_string()) } else { None }),
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
            let existing = find_existing_movie(db, library_id, tmdb_id_str, imdb_id_str).await?;
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

/// Fetch online_video metadata from download_records.
pub async fn fetch_online_record(
    db: &DatabaseConnection,
    library_id: Uuid,
    dir_path: &str,
) -> Result<Option<download_records::Model>, Box<dyn std::error::Error + Send + Sync>> {
    let dir_basename = dir_path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if dir_basename.is_empty() { return Ok(None); }
    let record = download_records::Entity::find()
        .filter(download_records::Column::TargetLibraryId.eq(library_id))
        .filter(download_records::Column::SourceOrigin.eq("online_media"))
        .filter(download_records::Column::TargetPath.contains(dir_basename))
        .order_by_desc(download_records::Column::CreatedAt)
        .one(db)
        .await?;
    Ok(record)
}

/// Download a remote thumbnail URL and upload to S3 as poster.
async fn download_thumbnail(
    state: &Arc<AppState>,
    url: &str,
    entity_kind: &str,
    entity_id: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = reqwest::get(url).await?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()).into());
    }
    let bytes = resp.bytes().await?;
    let ext = url
        .rsplit('.')
        .next()
        .and_then(|e| {
            let lower = e.split('?').next().unwrap_or(e).to_ascii_lowercase();
            if matches!(lower.as_str(), "jpg" | "jpeg" | "png" | "webp") { Some(lower) } else { None }
        })
        .unwrap_or_else(|| "jpg".to_string());
    let key = format!("library-images/{entity_kind}/{entity_id}/poster.{ext}");
    super::artwork::upload_image_buffer(state, &bytes, &key).await
}
