use rust_client_api::metadata_providers::tmdb::{TmdbClient, TmdbConfig};
use sea_orm::DatabaseConnection;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::AppState;

pub async fn handle(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    _job_id: Uuid,
    payload: &JsonValue,
) -> Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>> {
    let person_id = payload
        .get("personId")
        .and_then(|v| v.as_str())
        .ok_or("Missing personId")?;
    let person_uuid = Uuid::parse_str(person_id)?;

    let api_key = get_tmdb_api_key(db).await?;
    let Some(api_key) = api_key else {
        return Err("TMDB API Key 未配置".into());
    };

    use crate::db::entities::persons;
    use sea_orm::*;

    let person = persons::Entity::find_by_id(person_uuid).one(db).await?;
    let Some(person) = person else {
        warn!("[tmdb_person_scrape] Person {person_id} not found, skipping");
        return Ok(Some(json!({ "personId": person_id, "skipped": true })));
    };

    let tmdb_id = person.tmdb_id.as_deref();

    let client = TmdbClient::new(TmdbConfig {
        api_key,
        language: Some("zh-CN".to_string()),
        base_url: None,
        image_base_url: None,
        cache_ttl: None,
        http_client: state.http_client.clone(),
    });

    let tmdb_id_num: i64 = if let Some(tmdb_id) = tmdb_id {
        tmdb_id.parse()?
    } else {
        // No tmdb_id — search TMDB by name
        let results = client.search_person(&person.name).await?;
        let first = results
            .into_iter()
            .next()
            .ok_or_else(|| format!("TMDB search found no results for person '{}'", person.name))?;
        let found_id = first.id;

        // Persist tmdb_id so future scrapes skip the search
        let expr_val = sea_orm::prelude::Expr::value(found_id.to_string());
        persons::Entity::update_many()
            .col_expr(persons::Column::TmdbId, expr_val)
            .filter(persons::Column::Id.eq(person_uuid))
            .exec(db)
            .await?;

        found_id
    };

    debug!("[tmdb_person_scrape] Fetching TMDB person {tmdb_id_num} for {person_id}");
    let detail = client.get_person_detail(tmdb_id_num).await?;

    let now = chrono::Utc::now().fixed_offset();
    let mut active: persons::ActiveModel = person.into();

    active.name = Set(detail.name.clone());

    if let Some(bio) = &detail.biography {
        active.biography = Set(Some(bio.clone()));
    }
    if let Some(birthday_str) = &detail.birthday {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(birthday_str, "%Y-%m-%d") {
            active.birthday = Set(Some(date));
        }
    }
    if let Some(deathday_str) = &detail.deathday {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(deathday_str, "%Y-%m-%d") {
            active.deathday = Set(Some(date));
        }
    }
    if let Some(place) = &detail.place_of_birth {
        active.birthplace = Set(Some(place.clone()));
    }
    if let Some(popularity) = detail.popularity {
        active.popularity = Set(Some(popularity));
    }
    if let Some(dept) = &detail.known_for_department {
        active.known_for_dept = Set(Some(dept.clone()));
    }

    // Map TMDB gender int → string (0=unset, 1=female, 2=male, 3=non-binary)
    if let Some(g) = detail.gender {
        let label = match g {
            1 => Some("female"),
            2 => Some("male"),
            3 => Some("non-binary"),
            _ => None,
        };
        if let Some(label) = label {
            active.gender = Set(Some(label.to_string()));
        }
    }

    // Aliases from also_known_as
    if let Some(aka) = &detail.also_known_as {
        if !aka.is_empty() {
            active.aliases = Set(Some(aka.clone()));
        }
    }

    // IMDb ID from external_ids
    if let Some(ext) = &detail.external_ids {
        if let Some(imdb_id) = &ext.imdb_id {
            active.imdb_id = Set(Some(imdb_id.clone()));
        }
    }

    // Dispatch image_upload job for the profile image
    if let Some(profile_path) = &detail.profile_path {
        let storage_key = format!("tmdb-images/persons/{person_id}/profile.jpg");
        let tmdb_image_url = format!("https://image.tmdb.org/t/p/w500{profile_path}");

        crate::db::repos::job_repo::JobRepo::create_job(
            db,
            "image_upload",
            json!({
                "plexUrl": tmdb_image_url,
                "storageKey": storage_key,
                "entity": "person",
                "entityId": person_id,
                "field": "profilePath",
            }),
            None,
        )
        .await?;
    }

    active.updated_at = Set(Some(now));
    active.update(db).await?;

    // Broadcast event so frontend can refresh the person's page
    let movie_id = payload
        .get("movieId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let tv_show_id = payload
        .get("tvShowId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let _ = state.event_tx.send(crate::queue::AppEvent::PersonScraped {
        person_id: person_id.to_string(),
        movie_id,
        tv_show_id,
    });

    debug!("[tmdb_person_scrape] Updated person {person_id}");
    Ok(Some(json!({ "personId": person_id })))
}

/// Get TMDB API key from config.
async fn get_tmdb_api_key(
    db: &DatabaseConnection,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    use crate::db::repos::config_repo::{ConfigRepo, TmdbSettings};

    let setting = ConfigRepo::get::<TmdbSettings>(db).await?;
    Ok(setting.api_key)
}
