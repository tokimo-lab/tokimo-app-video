use rust_client_api::metadata_providers::tmdb::{TmdbClient, TmdbConfig};
use sea_orm::DatabaseConnection;
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde_json::{Value as JsonValue, json};
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

use crate::AppState;
use crate::queue::cancellation::{JobCancel, check_cancel};

pub async fn handle(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    _job_id: Uuid,
    payload: &JsonValue,
    cancel: &JobCancel,
) -> Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>> {
    check_cancel(cancel)?;
    let person_id = payload
        .get("personId")
        .and_then(|v| v.as_str())
        .ok_or("Missing personId")?;
    let person_uuid = Uuid::parse_str(person_id)?;

    // "movie" | "tv" — determines which table to read/write
    let person_type = payload.get("personType").and_then(|v| v.as_str()).unwrap_or("movie");

    let api_key = get_tmdb_api_key(db).await?;
    let Some(api_key) = api_key else {
        return Err("TMDB API Key 未配置".into());
    };

    // ── Fetch existing record from the correct table ──
    let (tmdb_id_opt, person_name) = if person_type == "tv" {
        use crate::db::entities::tv_persons;
        let Some(p) = tv_persons::Entity::find_by_id(person_uuid).one(db).await? else {
            warn!("[tmdb_person_scrape] TV person {person_id} not found, skipping");
            return Ok(Some(json!({ "personId": person_id, "skipped": true })));
        };
        (p.tmdb_id.clone(), p.name.clone())
    } else {
        use crate::db::entities::video_persons;
        let Some(p) = video_persons::Entity::find_by_id(person_uuid).one(db).await? else {
            warn!("[tmdb_person_scrape] Movie person {person_id} not found, skipping");
            return Ok(Some(json!({ "personId": person_id, "skipped": true })));
        };
        (p.tmdb_id.clone(), p.name.clone())
    };

    let client = TmdbClient::new(TmdbConfig {
        api_key,
        language: Some("zh-CN".to_string()),
        base_url: None,
        image_base_url: None,
        cache_ttl: None,
        http_client: state.http_client.clone(),
    });

    let tmdb_id_num: i64 = if let Some(tmdb_id) = tmdb_id_opt.as_deref() {
        tmdb_id.parse()?
    } else {
        check_cancel(cancel)?;
        let results = client.search_person(&person_name).await?;
        let first = results
            .into_iter()
            .next()
            .ok_or_else(|| format!("TMDB search found no results for person '{person_name}'"))?;
        let found_id = first.id;
        // Persist tmdb_id so future scrapes skip the search
        let tmdb_str = found_id.to_string();
        persist_tmdb_id(db, person_uuid, person_type, &tmdb_str).await?;
        found_id
    };

    // debug!("[tmdb_person_scrape] Fetching TMDB person {tmdb_id_num} for {person_id} ({person_type})");
    check_cancel(cancel)?;
    let detail = client.get_person_detail(tmdb_id_num).await?;

    // ── Dispatch image_upload job for the profile image ──
    if let Some(profile_path) = &detail.profile_path {
        let storage_key = format!("tmdb-images/persons/{person_id}/profile.jpg");
        let tmdb_image_url = format!("https://image.tmdb.org/t/p/w500{profile_path}");
        crate::db::repos::job_repo::JobRepo::create_job_via_bus(
            state,
            "image_upload",
            json!({
                "plexUrl": tmdb_image_url,
                "storageKey": storage_key,
                "entity": "person",
                "entityId": person_id,
                "personType": person_type,
                "field": "profilePath",
            }),
            None,
            None,
        )
        .await?;
    }

    // ── Write scraped data back to the correct table ──
    apply_person_detail(db, person_uuid, person_type, &detail).await?;

    let movie_id = payload.get("movieId").and_then(|v| v.as_str()).map(str::to_string);
    let tv_show_id = payload.get("tvShowId").and_then(|v| v.as_str()).map(str::to_string);
    let _ = state.event_tx.send(crate::queue::AppEvent::PersonScraped {
        person_id: person_id.to_string(),
        video_item_id: movie_id,
        tv_show_id,
    });

    // debug!("[tmdb_person_scrape] Updated {person_type} person {person_id}");
    Ok(Some(json!({ "personId": person_id })))
}

async fn persist_tmdb_id(
    db: &DatabaseConnection,
    person_uuid: Uuid,
    person_type: &str,
    tmdb_str: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if person_type == "tv" {
        use crate::db::entities::tv_persons;
        tv_persons::Entity::update_many()
            .col_expr(tv_persons::Column::TmdbId, Expr::value(tmdb_str))
            .col_expr(tv_persons::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(tv_persons::Column::Id.eq(person_uuid))
            .exec(db)
            .await?;
    } else {
        use crate::db::entities::video_persons;
        video_persons::Entity::update_many()
            .col_expr(video_persons::Column::TmdbId, Expr::value(tmdb_str))
            .col_expr(video_persons::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(video_persons::Column::Id.eq(person_uuid))
            .exec(db)
            .await?;
    }
    Ok(())
}

async fn apply_person_detail(
    db: &DatabaseConnection,
    person_uuid: Uuid,
    person_type: &str,
    detail: &rust_client_api::metadata_providers::tmdb::TmdbPersonDetail,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = chrono::Utc::now().fixed_offset();

    macro_rules! set_detail {
        ($active:expr) => {{
            $active.name = Set(detail.name.clone());
            if let Some(bio) = &detail.biography {
                $active.biography = Set(Some(bio.clone()));
            }
            if let Some(s) = &detail.birthday
                && let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            {
                $active.birthday = Set(Some(d));
            }
            if let Some(s) = &detail.deathday
                && let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            {
                $active.deathday = Set(Some(d));
            }
            if let Some(place) = &detail.place_of_birth {
                $active.birthplace = Set(Some(place.clone()));
            }
            if let Some(pop) = detail.popularity {
                $active.popularity = Set(Some(pop));
            }
            if let Some(dept) = &detail.known_for_department {
                $active.known_for_dept = Set(Some(dept.clone()));
            }
            if let Some(g) = detail.gender {
                let label = match g {
                    1 => Some("female"),
                    2 => Some("male"),
                    3 => Some("non-binary"),
                    _ => None,
                };
                if let Some(l) = label {
                    $active.gender = Set(Some(l.to_string()));
                }
            }
            if let Some(aka) = &detail.also_known_as
                && !aka.is_empty()
            {
                $active.aliases = Set(Some(aka.clone()));
            }
            // imdb_id is handled separately to avoid unique constraint violations
            $active.updated_at = Set(Some(now));
        }};
    }

    let new_imdb_id = detail.external_ids.as_ref().and_then(|ext| ext.imdb_id.clone());

    if person_type == "tv" {
        use crate::db::entities::tv_persons;
        let p = tv_persons::Entity::find_by_id(person_uuid).one(db).await?;
        let Some(p) = p else { return Ok(()) };
        let mut active: tv_persons::ActiveModel = p.into();
        set_detail!(active);
        if let Some(ref imdb_id) = new_imdb_id {
            let conflict = tv_persons::Entity::find()
                .filter(tv_persons::Column::ImdbId.eq(imdb_id.as_str()))
                .filter(tv_persons::Column::Id.ne(person_uuid))
                .count(db)
                .await?;
            if conflict == 0 {
                active.imdb_id = Set(Some(imdb_id.clone()));
            } else {
                warn!(
                    "[tmdb_person_scrape] Skipping imdb_id {imdb_id} for tv_person {person_uuid}: already used by another record"
                );
            }
        }
        active.update(db).await?;
    } else {
        use crate::db::entities::video_persons;
        let p = video_persons::Entity::find_by_id(person_uuid).one(db).await?;
        let Some(p) = p else { return Ok(()) };
        let mut active: video_persons::ActiveModel = p.into();
        set_detail!(active);
        if let Some(ref imdb_id) = new_imdb_id {
            let conflict = video_persons::Entity::find()
                .filter(video_persons::Column::ImdbId.eq(imdb_id.as_str()))
                .filter(video_persons::Column::Id.ne(person_uuid))
                .count(db)
                .await?;
            if conflict == 0 {
                active.imdb_id = Set(Some(imdb_id.clone()));
            } else {
                warn!(
                    "[tmdb_person_scrape] Skipping imdb_id {imdb_id} for video_person {person_uuid}: already used by another record"
                );
            }
        }
        active.update(db).await?;
    }
    Ok(())
}

async fn get_tmdb_api_key(db: &DatabaseConnection) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    use crate::config::TmdbSettings;
    use crate::db::repos::system_config_repo::SystemConfigRepo;
    let setting = SystemConfigRepo::get::<TmdbSettings>(db).await?;
    Ok(setting.api_key)
}
