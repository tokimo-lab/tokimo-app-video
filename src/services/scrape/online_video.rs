//! Online video movie creation — dedicated branch for `online_video` libraries.
//!
//! Metadata priority: NFO (written by download pipeline) → `download_records` → dir name.
//! No TMDB scraping.

use chrono::Datelike;
use sea_orm::prelude::Expr;
use sea_orm::sea_query::Condition;
use sea_orm::*;
use serde_json::json;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use crate::AppState;
use crate::db::entities::{download_records, video_items};

use crate::queue::handlers::common::is_unique_violation;
use crate::queue::handlers::nfo_parser::NfoInfo;
use crate::services::media::scrape::shared::artwork::{
    DiscoveredArtwork, upload_extra_art, upload_poster_and_backdrop,
};

pub struct OnlineVideoResult {
    pub video_item_id: Uuid,
}

/// Fetch matching `download_record` for an `online_video` file by directory name.
///
/// Matches by `external_id = dir_basename` (BV number, `YouTube` ID, etc.)
/// OR `target_path LIKE %dir_basename%` as fallback.
pub async fn fetch_online_record(
    db: &DatabaseConnection,
    app_id: Uuid,
    dir_path: &str,
) -> Result<Option<download_records::Model>, Box<dyn std::error::Error + Send + Sync>> {
    let dir_basename = dir_path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if dir_basename.is_empty() {
        return Ok(None);
    }
    let record = download_records::Entity::find()
        .filter(download_records::Column::TargetVideoId.eq(app_id))
        .filter(download_records::Column::SourceOrigin.eq("online_media"))
        .filter(
            Condition::any()
                .add(download_records::Column::ExternalId.eq(dir_basename))
                .add(download_records::Column::TargetPath.contains(dir_basename)),
        )
        .order_by_desc(download_records::Column::CreatedAt)
        .one(db)
        .await?;
    Ok(record)
}

/// Find or create a movie record for an `online_video` library.
///
/// Metadata priority: NFO → `download_records` → directory name.
/// No TMDB. Artwork from local files → remote thumbnail fallback.
#[allow(clippy::too_many_arguments)]
pub async fn find_or_create_online_video(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    app_id: Uuid,
    nfo: Option<&NfoInfo>,
    online_record: Option<&download_records::Model>,
    dir_folder_name: &str,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb_path: Option<&str>,
    nfo_backdrop_tmdb_path: Option<&str>,
) -> Result<OnlineVideoResult, Box<dyn std::error::Error + Send + Sync>> {
    // Title: NFO → download_records → dir name
    let nfo_title = nfo.and_then(|n| n.title.clone());
    let record_title = online_record.and_then(|r| r.media_title.clone());
    let title = nfo_title
        .or(record_title)
        .unwrap_or_else(|| dir_folder_name.to_string());

    // Advisory lock to prevent duplicate creation
    let lock_key = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        app_id.hash(&mut h);
        title.to_lowercase().hash(&mut h);
        h.finish() as i64
    };

    let txn = db.begin().await?;
    txn.execute_unprepared(&format!("SELECT pg_advisory_xact_lock({lock_key})"))
        .await?;

    // Dedup by title within the same library
    let existing = find_existing(db, app_id, &title).await?;

    let movie_id = if let Some(id) = existing {
        // Backfill metadata/year/overview if not yet populated (e.g. prior scan had no download_record match)
        let needs_metadata = nfo.is_some() || online_record.is_some();
        if needs_metadata {
            let current = video_items::Entity::find_by_id(id).one(db).await?;
            if let Some(ref m) = current {
                let missing_metadata = m.metadata.is_none();
                let missing_year = m.year.is_none();
                let missing_overview = m.overview.is_none();
                if missing_metadata || missing_year || missing_overview {
                    let mut update = video_items::Entity::update_many()
                        .col_expr(video_items::Column::UpdatedAt, Expr::cust("NOW()"))
                        .col_expr(video_items::Column::ScrapedAt, Expr::cust("NOW()"))
                        .filter(video_items::Column::Id.eq(id));

                    if missing_metadata && let Some(mj) = build_metadata_json(nfo, online_record) {
                        update = update.col_expr(video_items::Column::Metadata, Expr::value(mj));
                    }
                    if missing_year {
                        let snapshot_date = online_record.and_then(|r| r.analysis_snapshot.as_ref()).and_then(|s| {
                            s.get("releaseDate")
                                .or_else(|| s.get("release_date"))
                                .and_then(|v| v.as_str())
                                .and_then(normalize_date_to_naive)
                                .or_else(|| {
                                    s.get("rawMetadata")
                                        .and_then(|rm| rm.get("upload_date"))
                                        .and_then(|v| v.as_str())
                                        .and_then(normalize_date_to_naive)
                                })
                        });
                        if let Some(d) = snapshot_date {
                            update = update.col_expr(video_items::Column::Year, Expr::value(d.year()));
                            update = update.col_expr(video_items::Column::ReleaseDate, Expr::value(d));
                        }
                    }
                    if missing_overview {
                        let overview = nfo.and_then(|n| n.plot.clone()).or_else(|| {
                            online_record.and_then(|r| {
                                r.analysis_snapshot
                                    .as_ref()
                                    .and_then(|s| s.get("description"))
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.is_empty())
                                    .map(String::from)
                            })
                        });
                        if let Some(ov) = overview {
                            update = update.col_expr(video_items::Column::Overview, Expr::value(ov));
                        }
                    }

                    update.exec(&txn).await?;
                    txn.commit().await?;
                    return upload_artwork_and_finish(
                        db,
                        state,
                        id,
                        app_id,
                        nfo,
                        online_record,
                        artwork,
                        nfo_poster_tmdb_path,
                        nfo_backdrop_tmdb_path,
                    )
                    .await;
                }
            }
        }

        video_items::Entity::update_many()
            .col_expr(video_items::Column::UpdatedAt, Expr::cust("NOW()"))
            .col_expr(video_items::Column::ScrapedAt, Expr::cust("NOW()"))
            .filter(video_items::Column::Id.eq(id))
            .exec(&txn)
            .await?;
        txn.commit().await?;
        id
    } else {
        let id = create_record(&txn, app_id, &title, nfo, online_record).await?;
        txn.commit().await?;
        id
    };

    upload_artwork_and_finish(
        db,
        state,
        movie_id,
        app_id,
        nfo,
        online_record,
        artwork,
        nfo_poster_tmdb_path,
        nfo_backdrop_tmdb_path,
    )
    .await
}

async fn find_existing(
    db: &impl ConnectionTrait,
    app_id: Uuid,
    title: &str,
) -> Result<Option<Uuid>, Box<dyn std::error::Error + Send + Sync>> {
    if title.is_empty() {
        return Ok(None);
    }
    Ok(video_items::Entity::find()
        .filter(video_items::Column::VideoId.eq(app_id))
        .filter(video_items::Column::Title.eq(title))
        .one(db)
        .await?
        .map(|m| m.id))
}

/// Create a movie record from NFO + `download_records` metadata.
async fn create_record(
    db: &impl ConnectionTrait,
    app_id: Uuid,
    title: &str,
    nfo: Option<&NfoInfo>,
    online_record: Option<&download_records::Model>,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    let movie_id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();

    let original_title = nfo
        .and_then(|n| n.original_title.clone())
        .or_else(|| online_record.and_then(|r| r.media_title.clone()));

    // Overview: NFO plot → download_records.analysis_snapshot.description
    let overview = nfo.and_then(|n| n.plot.clone()).or_else(|| {
        online_record.and_then(|r| {
            r.analysis_snapshot
                .as_ref()
                .and_then(|s| s.get("description"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
        })
    });

    let runtime = nfo.and_then(|n| n.runtime).or_else(|| {
        online_record
            .and_then(|r| r.duration_seconds)
            .map(|s| (f64::from(s) / 60.0).round() as i32)
    });

    // Year & release_date: NFO → download_records.analysis_snapshot.releaseDate / upload_date
    let (year, release_date) = {
        let nfo_year = nfo.and_then(|n| n.year);
        let nfo_date = nfo.and_then(|n| {
            n.release_date
                .as_deref()
                .and_then(|r| chrono::NaiveDate::parse_from_str(r, "%Y-%m-%d").ok())
        });
        if nfo_year.is_some() || nfo_date.is_some() {
            (nfo_year, nfo_date)
        } else {
            // Fallback: extract from analysis_snapshot.releaseDate or raw upload_date
            let snapshot_date = online_record.and_then(|r| r.analysis_snapshot.as_ref()).and_then(|s| {
                s.get("releaseDate")
                    .or_else(|| s.get("release_date"))
                    .and_then(|v| v.as_str())
                    .and_then(normalize_date_to_naive)
                    .or_else(|| {
                        // Try rawMetadata.upload_date (yt-dlp YYYYMMDD format)
                        s.get("rawMetadata")
                            .and_then(|rm| rm.get("upload_date"))
                            .and_then(|v| v.as_str())
                            .and_then(normalize_date_to_naive)
                    })
            });
            let year = snapshot_date.map(|d| d.year());
            (year, snapshot_date)
        }
    };
    let tagline = nfo.and_then(|n| n.tagline.clone());
    let content_rating = nfo.and_then(|n| n.content_rating.clone());

    let metadata_json = build_metadata_json(nfo, online_record);

    let model = video_items::ActiveModel {
        id: Set(movie_id),
        video_id: Set(app_id),
        title: Set(title.to_string()),
        original_title: Set(original_title),
        sort_title: Set(None),
        year: Set(year),
        release_date: Set(release_date),
        runtime: Set(runtime),
        tmdb_rating: Set(None),
        imdb_rating: Set(None),
        douban_rating: Set(None),
        tmdb_id: Set(None),
        imdb_id: Set(None),
        douban_id: Set(None),
        jav_number: Set(None),
        javbus_id: Set(None),
        javdb_id: Set(None),
        poster_path: Set(None),
        backdrop_path: Set(None),
        overview: Set(overview),
        tagline: Set(tagline),
        is_adult: Set(false),
        is_favorite: Set(false),
        original_language: Set(None),
        countries: Set(None),
        spoken_languages: Set(None),
        content_rating: Set(content_rating),
        content_advisories: Set(None),
        locked_fields: Set(None),
        metadata: Set(metadata_json),
        scraped_at: Set(Some(now)),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
    };

    match video_items::Entity::insert(model).exec(db).await {
        Ok(_) => {
            info!("[online_video] Created movie: {title}");
            Ok(movie_id)
        }
        Err(e) if is_unique_violation(&e) => {
            let existing = find_existing(db, app_id, title).await?;
            if let Some(id) = existing {
                info!("[online_video] Movie already exists (concurrent): {title}");
                Ok(id)
            } else {
                Err(e.into())
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Parse date strings: "YYYYMMDD" (yt-dlp) or "YYYY-MM-DD" → `NaiveDate`
fn normalize_date_to_naive(s: &str) -> Option<chrono::NaiveDate> {
    let t = s.trim();
    if t.len() == 8 && t.chars().all(|c| c.is_ascii_digit()) {
        let formatted = format!("{}-{}-{}", &t[0..4], &t[4..6], &t[6..8]);
        return chrono::NaiveDate::parse_from_str(&formatted, "%Y-%m-%d").ok();
    }
    chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d").ok()
}

/// Build metadata JSON from NFO and/or `download_records`.
fn build_metadata_json(
    nfo: Option<&NfoInfo>,
    online_record: Option<&download_records::Model>,
) -> Option<serde_json::Value> {
    let mut metadata = serde_json::Map::new();
    if let Some(nfo) = nfo {
        if let Some(ref s) = nfo.studio {
            metadata.insert("sourceSite".into(), json!(s));
        }
        if let Some(ref c) = nfo.country {
            metadata.insert("country".into(), json!(c));
        }
        if let Some(uploader) = nfo.directors.first() {
            metadata.insert("uploader".into(), json!(uploader));
        }
    }
    if let Some(or) = online_record {
        if !metadata.contains_key("uploader")
            && let Some(ref u) = or.uploader
        {
            metadata.insert("uploader".into(), json!(u));
        }
        if !metadata.contains_key("sourceSite")
            && let Some(ref s) = or.source_site
        {
            metadata.insert("sourceSite".into(), json!(s));
        }
        if let Some(ref e) = or.external_id {
            metadata.insert("externalId".into(), json!(e));
        }
        if let Some(ref url) = or.source_url {
            metadata.insert("sourceUrl".into(), json!(url));
        }
        if let Some(dur) = or.duration_seconds {
            metadata.insert("durationSeconds".into(), json!(dur));
        }
    }
    if metadata.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(metadata))
    }
}

/// Upload artwork and return the final result (shared by new + existing paths).
#[allow(clippy::too_many_arguments)]
async fn upload_artwork_and_finish(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    movie_id: Uuid,
    _app_id: Uuid,
    nfo: Option<&NfoInfo>,
    online_record: Option<&download_records::Model>,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb_path: Option<&str>,
    nfo_backdrop_tmdb_path: Option<&str>,
) -> Result<OnlineVideoResult, Box<dyn std::error::Error + Send + Sync>> {
    let (mut poster_path, backdrop_path) = upload_poster_and_backdrop(
        db,
        state,
        "movie",
        movie_id,
        artwork,
        nfo_poster_tmdb_path,
        nfo_backdrop_tmdb_path,
        None,
        None,
    )
    .await?;

    if poster_path.is_none() {
        let thumb_url = nfo
            .and_then(|n| n.poster_url.as_deref())
            .filter(|u| u.starts_with("http"))
            .or_else(|| online_record.and_then(|r| r.thumbnail_url.as_deref()));
        if let Some(url) = thumb_url
            && !url.is_empty()
        {
            match download_thumbnail(state, url, &movie_id.to_string()).await {
                Ok(sp) => poster_path = Some(sp),
                Err(e) => tracing::warn!("[online_video] thumbnail download failed: {e}"),
            }
        }
    }

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

    upload_extra_art(db, state, Some(movie_id), None, &artwork.extra_art).await?;

    Ok(OnlineVideoResult {
        video_item_id: movie_id,
    })
}

/// Download a remote thumbnail URL and upload to local storage as poster.
async fn download_thumbnail(
    state: &Arc<AppState>,
    url: &str,
    entity_id: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = state.http_client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()).into());
    }
    let bytes = resp.bytes().await?;
    let ext = url
        .rsplit('.')
        .next()
        .and_then(|e| {
            let lower = e.split('?').next().unwrap_or(e).to_ascii_lowercase();
            if matches!(lower.as_str(), "jpg" | "jpeg" | "png" | "webp") {
                Some(lower)
            } else {
                None
            }
        })
        .unwrap_or_else(|| "jpg".to_string());
    let key = format!("library-images/movies/{entity_id}/poster.{ext}");
    crate::services::media::scrape::shared::artwork::upload_image_buffer(&state.storage, &bytes, &key).await
}
