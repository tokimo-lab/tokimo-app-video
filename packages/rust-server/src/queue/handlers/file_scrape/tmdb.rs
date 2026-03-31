//! TMDB scraping utilities: API key, candidate scoring, movie/TV detail fetch.

use rust_client_api::metadata_providers::tmdb::{TmdbClient, TmdbConfig, TmdbMedia, TmdbMediaDetail};
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use tracing::warn;

use crate::AppState;
use super::artwork::DiscoveredArtwork;
use super::nfo_parser::NfoInfo;

pub async fn get_api_key(db: &DatabaseConnection) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    use crate::db::repos::config_repo::{ConfigRepo, TmdbSettings};
    let setting = ConfigRepo::get::<TmdbSettings>(db).await?;
    if setting.api_key.is_some() {
        return Ok(setting.api_key);
    }
    Ok(std::env::var("TMDB_API_KEY").ok().filter(|k| !k.is_empty()))
}

pub fn build_client(state: &Arc<AppState>, api_key: &str) -> TmdbClient {
    TmdbClient::new(TmdbConfig {
        api_key: api_key.to_string(),
        language: Some("zh-CN".to_string()),
        base_url: None,
        image_base_url: None,
        cache_ttl: None,
        http_client: state.http_client.clone(),
    })
}

/// Score a TMDB candidate against parsed title/year.
fn score_candidate(
    candidate: &TmdbMedia,
    title: &str,
    year: Option<i32>,
    is_tv: bool,
) -> f64 {
    let mut score = 0.0f64;
    let parsed_lower = title.to_lowercase();
    let cand_lower = candidate.title.to_lowercase();
    let orig_lower = candidate
        .original_title
        .as_deref()
        .unwrap_or("")
        .to_lowercase();

    if cand_lower == parsed_lower || orig_lower == parsed_lower {
        score += 100.0;
    } else if cand_lower.contains(&parsed_lower) || parsed_lower.contains(&cand_lower) {
        score += 60.0;
    } else if !orig_lower.is_empty()
        && (orig_lower.contains(&parsed_lower) || parsed_lower.contains(&orig_lower))
    {
        score += 50.0;
    }

    if let (Some(py), Some(rd)) = (year, &candidate.release_date) {
        if let Ok(cy) = rd.get(..4).unwrap_or("").parse::<i32>() {
            if cy == py {
                score += 30.0;
            } else if (cy - py).abs() <= 1 {
                score += 15.0;
            }
        }
    }

    let expected = if is_tv { "tv" } else { "movie" };
    if candidate.media_type == expected {
        score += 20.0;
    } else {
        score -= 10.0;
    }

    if let Some(pop) = candidate.popularity {
        score += (pop / 10.0).min(10.0);
    }
    score
}

/// Pick the best TMDB match from candidates using scoring.
fn pick_best_match(
    candidates: &[TmdbMedia],
    title: &str,
    year: Option<i32>,
    is_tv: bool,
) -> Option<i64> {
    if candidates.is_empty() {
        return None;
    }
    let mut scored: Vec<(usize, f64)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (i, score_candidate(c, title, year, is_tv)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if scored.len() == 1
        || (scored[0].1 >= 80.0 && scored[0].1 - scored.get(1).map_or(0.0, |s| s.1) >= 30.0)
    {
        return Some(candidates[scored[0].0].id);
    }
    Some(candidates[scored[0].0].id)
}

/// TMDB movie scrape: NFO ID priority → search with scoring.
pub async fn scrape_movie(
    tmdb: &TmdbClient,
    nfo: &Option<NfoInfo>,
    title: &str,
    year: Option<i32>,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb: &Option<String>,
    nfo_backdrop_tmdb: &Option<String>,
) -> Option<TmdbMediaDetail> {
    let needs_remote_art = artwork.poster_buf.is_none() && nfo_poster_tmdb.is_none()
        || artwork.fanart_buf.is_none() && nfo_backdrop_tmdb.is_none();

    if let Some(nfo) = nfo {
        if let Some(ref tmdb_id) = nfo.tmdb_id {
            if let Ok(id) = tmdb_id.parse::<i64>() {
                if nfo.is_sufficient() && !needs_remote_art {
                    return None;
                }
                return tmdb.get_movie_detail(id).await.ok();
            }
        }
        if let Some(ref imdb_id) = nfo.imdb_id {
            if let Ok(Some(found)) = tmdb.find_by_imdb_id(imdb_id).await {
                return tmdb.get_movie_detail(found.id).await.ok();
            }
        }
    }

    let mut candidates = tmdb
        .search_movies(title, year.map(|y| y as u32), 1)
        .await
        .ok()
        .unwrap_or_default();

    if candidates.is_empty() {
        candidates = tmdb.search_multi(title, 1).await.ok().unwrap_or_default();
    }
    if candidates.is_empty() && year.is_some() {
        candidates = tmdb.search_movies(title, None, 1).await.ok().unwrap_or_default();
        if candidates.is_empty() {
            candidates = tmdb.search_multi(title, 1).await.ok().unwrap_or_default();
        }
    }

    if let Some(best_id) = pick_best_match(&candidates, title, year, false) {
        return tmdb.get_movie_detail(best_id).await.ok();
    }

    warn!("[file_scrape] No TMDB match for movie: {title} ({year:?})");
    None
}

/// TMDB TV scrape: NFO ID priority → search with scoring.
pub async fn scrape_tv(
    tmdb: &TmdbClient,
    nfo: &Option<NfoInfo>,
    title: &str,
    year: Option<i32>,
    artwork: &DiscoveredArtwork,
    nfo_poster_tmdb: &Option<String>,
    nfo_backdrop_tmdb: &Option<String>,
) -> Option<TmdbMediaDetail> {
    let needs_remote_art = artwork.poster_buf.is_none() && nfo_poster_tmdb.is_none()
        || artwork.fanart_buf.is_none() && nfo_backdrop_tmdb.is_none();

    if let Some(nfo) = nfo {
        if let Some(ref tmdb_id) = nfo.tmdb_id {
            if let Ok(id) = tmdb_id.parse::<i64>() {
                if nfo.is_sufficient() && !needs_remote_art {
                    return None;
                }
                return tmdb.get_tv_detail(id).await.ok();
            }
        }
        if let Some(ref imdb_id) = nfo.imdb_id {
            if let Ok(Some(found)) = tmdb.find_by_imdb_id(imdb_id).await {
                return tmdb.get_tv_detail(found.id).await.ok();
            }
        }
    }

    let mut candidates = tmdb
        .search_tv(title, year.map(|y| y as u32), 1)
        .await
        .ok()
        .unwrap_or_default();

    if candidates.is_empty() {
        candidates = tmdb.search_multi(title, 1).await.ok().unwrap_or_default();
    }
    if candidates.is_empty() && year.is_some() {
        candidates = tmdb.search_tv(title, None, 1).await.ok().unwrap_or_default();
        if candidates.is_empty() {
            candidates = tmdb.search_multi(title, 1).await.ok().unwrap_or_default();
        }
    }

    if let Some(best_id) = pick_best_match(&candidates, title, year, true) {
        return tmdb.get_tv_detail(best_id).await.ok();
    }

    warn!("[file_scrape] No TMDB match for TV: {title} ({year:?})");
    None
}
