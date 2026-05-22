use crate::db::{ApiDateTimeExt, OptionalApiDateTimeExt};
use chrono::Utc;
use sea_orm::*;
use serde::Serialize;
use tracing::error;
use uuid::Uuid;

use crate::db::entities::{subscription_filters, subscriptions};
use crate::error::AppError;

// ── DTO ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionDto {
    pub id: String,
    pub subscription_mode: String,
    pub media_type: String,
    pub tmdb_id: Option<i64>,
    pub title: String,
    pub year: Option<String>,
    pub poster_path: Option<String>,
    pub season: Option<i32>,
    pub episodes: Option<Vec<i32>>,
    pub series_prefix: Option<String>,
    pub metadata_source: Option<String>,
    pub max_downloads_per_run: i32,
    pub filter_ids: Option<Vec<String>>,
    pub filter_names: Option<Vec<String>>,
    pub filter_overrides: Option<serde_json::Value>,
    pub status: String,
    pub interval_minutes: i32,
    pub site_ids: Option<Vec<String>>,
    pub download_client_id: Option<String>,
    pub target_app_id: Option<String>,
    pub last_checked_at: Option<String>,
    pub next_check_at: Option<String>,
    pub created_by: Option<String>,
    pub created_by_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeProgress {
    pub downloaded_episodes: Vec<i32>,
    pub total_episodes: Option<i32>,
}

// ── Input types ─────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSubscriptionInput {
    pub subscription_mode: Option<String>,
    pub media_type: String,
    pub tmdb_id: Option<i64>,
    pub title: String,
    pub year: Option<String>,
    pub poster_path: Option<String>,
    pub season: Option<i32>,
    pub episodes: Option<Vec<i32>>,
    pub series_prefix: Option<String>,
    pub metadata_source: Option<String>,
    pub max_downloads_per_run: Option<i32>,
    pub filter_ids: Option<Vec<String>>,
    pub filter_overrides: Option<serde_json::Value>,
    pub interval_minutes: Option<i32>,
    pub site_ids: Option<Vec<String>>,
    pub download_client_id: Option<String>,
    pub target_app_id: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubscriptionInput {
    #[serde(default)]
    pub id: String,
    pub season: Option<Option<i32>>,
    pub episodes: Option<Option<Vec<i32>>>,
    pub filter_ids: Option<Option<Vec<String>>>,
    pub filter_overrides: Option<Option<serde_json::Value>>,
    pub status: Option<String>,
    pub interval_minutes: Option<i32>,
    pub max_downloads_per_run: Option<i32>,
    pub site_ids: Option<Option<Vec<String>>>,
    pub download_client_id: Option<Option<String>>,
    pub target_app_id: Option<Option<String>>,
}

// ── Conversion helpers ──────────────────────────────────────────────────────

fn parse_json_string_vec(val: &Option<serde_json::Value>) -> Option<Vec<String>> {
    val.as_ref().and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn parse_json_int_vec(val: &Option<serde_json::Value>) -> Option<Vec<i32>> {
    val.as_ref().and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn collect_filter_ids(model: &subscriptions::Model) -> Vec<String> {
    let from_json = parse_json_string_vec(&model.filter_ids);
    if let Some(ids) = from_json
        && !ids.is_empty()
    {
        return ids;
    }
    if let Some(fid) = model.filter_id {
        return vec![fid.to_string()];
    }
    vec![]
}

fn to_dto(
    model: &subscriptions::Model,
    filter_names: Option<Vec<String>>,
    created_by_name: Option<String>,
) -> SubscriptionDto {
    SubscriptionDto {
        id: model.id.to_string(),
        subscription_mode: model.subscription_mode.clone(),
        media_type: model.media_type.clone(),
        tmdb_id: model.tmdb_id.as_ref().and_then(|s| s.parse::<i64>().ok()),
        title: model.title.clone(),
        year: model.year.clone(),
        poster_path: model.poster_path.clone(),
        season: model.season.as_ref().and_then(|s| s.parse::<i32>().ok()),
        episodes: parse_json_int_vec(&model.episodes),
        series_prefix: model.series_prefix.clone(),
        metadata_source: model.metadata_source.clone(),
        max_downloads_per_run: model.max_downloads_per_run,
        filter_ids: {
            let from_json = parse_json_string_vec(&model.filter_ids);
            if from_json.as_ref().is_some_and(|v| !v.is_empty()) {
                from_json
            } else {
                model.filter_id.map(|id| vec![id.to_string()])
            }
        },
        filter_names,
        filter_overrides: model.filter_overrides.clone(),
        status: model.status.clone(),
        interval_minutes: model.interval_minutes.parse::<i32>().unwrap_or(30),
        site_ids: parse_json_string_vec(&model.site_ids),
        download_client_id: model.download_client_id.map(|id| id.to_string()),
        target_app_id: model.target_video_id.map(|id| id.to_string()),
        last_checked_at: model.last_checked_at.to_api_datetime(),
        next_check_at: model.next_check_at.to_api_datetime(),
        created_by: model.created_by.map(|id| id.to_string()),
        created_by_name,
        created_at: model.created_at.to_api_datetime_or_default(),
        updated_at: model.updated_at.to_api_datetime_or_default(),
    }
}

// ── Repo ────────────────────────────────────────────────────────────────────

pub struct SubscriptionRepo;

impl SubscriptionRepo {
    async fn resolve_filter_names<C: ConnectionTrait>(db: &C, filter_ids: &[String]) -> Vec<String> {
        if filter_ids.is_empty() {
            return vec![];
        }
        let uuids: Vec<Uuid> = filter_ids.iter().filter_map(|id| id.parse::<Uuid>().ok()).collect();
        if uuids.is_empty() {
            return filter_ids.to_vec();
        }
        match subscription_filters::Entity::find()
            .filter(subscription_filters::Column::Id.is_in(uuids))
            .all(db)
            .await
        {
            Ok(filters) => {
                let name_map: std::collections::HashMap<String, String> =
                    filters.into_iter().map(|f| (f.id.to_string(), f.name)).collect();
                filter_ids
                    .iter()
                    .map(|id| name_map.get(id).cloned().unwrap_or_else(|| id.clone()))
                    .collect()
            }
            Err(e) => {
                error!("resolve_filter_names failed: {e}");
                filter_ids.to_vec()
            }
        }
    }

    pub async fn list<C: ConnectionTrait>(db: &C, user_id: &str) -> Result<Vec<SubscriptionDto>, AppError> {
        let uid: Uuid = user_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid user id".into()))?;

        let rows = subscriptions::Entity::find()
            .filter(subscriptions::Column::CreatedBy.eq(uid))
            .order_by_desc(subscriptions::Column::CreatedAt)
            .all(db)
            .await?;

        // Batch resolve filter names
        let mut all_filter_ids = std::collections::HashSet::new();
        for sub in &rows {
            for id in collect_filter_ids(sub) {
                all_filter_ids.insert(id);
            }
        }
        let all_ids_vec: Vec<String> = all_filter_ids.into_iter().collect();
        let uuids: Vec<Uuid> = all_ids_vec.iter().filter_map(|id| id.parse::<Uuid>().ok()).collect();
        let filter_name_map: std::collections::HashMap<String, String> = if uuids.is_empty() {
            std::collections::HashMap::new()
        } else {
            match subscription_filters::Entity::find()
                .filter(subscription_filters::Column::Id.is_in(uuids))
                .all(db)
                .await
            {
                Ok(filters) => filters.into_iter().map(|f| (f.id.to_string(), f.name)).collect(),
                Err(e) => {
                    error!("batch resolve filter names failed: {e}");
                    std::collections::HashMap::new()
                }
            }
        };

        Ok(rows
            .iter()
            .map(|sub| {
                let fids = collect_filter_ids(sub);
                let fnames: Vec<String> = fids
                    .iter()
                    .map(|id| filter_name_map.get(id).cloned().unwrap_or_else(|| id.clone()))
                    .collect();
                to_dto(
                    sub,
                    if fnames.is_empty() { None } else { Some(fnames) },
                    None,
                )
            })
            .collect())
    }

    pub async fn get_by_id<C: ConnectionTrait>(
        db: &C,
        id: &str,
    ) -> Result<Option<SubscriptionDto>, AppError> {
        let uid: Uuid = id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid subscription id".into()))?;

        let row = subscriptions::Entity::find_by_id(uid)
            .one(db)
            .await?;

        match row {
            Some(sub) => {
                let fids = collect_filter_ids(&sub);
                let fnames = Self::resolve_filter_names(db, &fids).await;
                let dto = to_dto(
                    &sub,
                    if fnames.is_empty() { None } else { Some(fnames) },
                    None,
                );
                Ok(Some(dto))
            }
            None => Ok(None),
        }
    }

    pub async fn get_raw<C: ConnectionTrait>(
        db: &C,
        id: &str,
    ) -> Result<Option<subscriptions::Model>, AppError> {
        let uid: Uuid = id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid subscription id".into()))?;
        Ok(subscriptions::Entity::find_by_id(uid).one(db).await?)
    }

    pub async fn create<C: ConnectionTrait>(
        db: &C,
        input: CreateSubscriptionInput,
        user_id: &str,
    ) -> Result<SubscriptionDto, AppError> {
        let user_uuid: Uuid = user_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid user id".into()))?;
        let now = Utc::now().fixed_offset();

        let active = subscriptions::ActiveModel {
            id: Set(Uuid::new_v4()),
            subscription_mode: Set(input.subscription_mode.unwrap_or_else(|| "tmdb".to_string())),
            media_type: Set(input.media_type),
            tmdb_id: Set(input.tmdb_id.map(|n| n.to_string())),
            title: Set(input.title),
            year: Set(input.year),
            poster_path: Set(input.poster_path),
            season: Set(input.season.map(|n| n.to_string())),
            episodes: Set(input.episodes.map(|e| serde_json::to_value(e).unwrap_or_default())),
            series_prefix: Set(input.series_prefix),
            metadata_source: Set(input.metadata_source),
            max_downloads_per_run: Set(input.max_downloads_per_run.unwrap_or(10)),
            filter_id: Set(input
                .filter_ids
                .as_ref()
                .and_then(|ids| ids.first())
                .and_then(|id| id.parse::<Uuid>().ok())),
            filter_ids: Set(input
                .filter_ids
                .as_ref()
                .map(|ids| serde_json::to_value(ids).unwrap_or_default())),
            filter_overrides: Set(input.filter_overrides),
            status: Set("active".to_string()),
            interval_minutes: Set(input.interval_minutes.unwrap_or(30).to_string()),
            site_ids: Set(input
                .site_ids
                .as_ref()
                .map(|ids| serde_json::to_value(ids).unwrap_or_default())),
            download_client_id: Set(input.download_client_id.as_ref().and_then(|id| id.parse::<Uuid>().ok())),
            target_video_id: Set(input.target_app_id.as_ref().and_then(|id| id.parse::<Uuid>().ok())),
            last_checked_at: Set(None),
            next_check_at: Set(Some(now)),
            created_by: Set(Some(user_uuid)),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
        };

        let model = subscriptions::Entity::insert(active).exec_with_returning(db).await?;
        let fids = collect_filter_ids(&model);
        let fnames = Self::resolve_filter_names(db, &fids).await;
        Ok(to_dto(
            &model,
            if fnames.is_empty() { None } else { Some(fnames) },
            None,
        ))
    }

    pub async fn update<C: ConnectionTrait>(
        db: &C,
        id: &str,
        input: UpdateSubscriptionInput,
    ) -> Result<Option<SubscriptionDto>, AppError> {
        let uid: Uuid = id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid subscription id".into()))?;

        let Some(existing) = subscriptions::Entity::find_by_id(uid).one(db).await? else {
            return Ok(None);
        };

        let mut active: subscriptions::ActiveModel = existing.into();
        let now = Utc::now().fixed_offset();
        active.updated_at = Set(Some(now));

        if let Some(season) = input.season {
            active.season = Set(season.map(|n| n.to_string()));
        }
        if let Some(episodes) = input.episodes {
            active.episodes = Set(episodes.map(|e| serde_json::to_value(e).unwrap_or_default()));
        }
        if let Some(filter_ids) = &input.filter_ids {
            active.filter_ids = Set(filter_ids
                .as_ref()
                .map(|ids| serde_json::to_value(ids).unwrap_or_default()));
            active.filter_id = Set(filter_ids
                .as_ref()
                .and_then(|ids| ids.first())
                .and_then(|id| id.parse::<Uuid>().ok()));
        }
        if let Some(filter_overrides) = input.filter_overrides {
            active.filter_overrides = Set(filter_overrides);
        }
        if let Some(status) = input.status {
            active.status = Set(status);
        }
        if let Some(interval_minutes) = input.interval_minutes {
            active.interval_minutes = Set(interval_minutes.to_string());
        }
        if let Some(max_downloads) = input.max_downloads_per_run {
            active.max_downloads_per_run = Set(max_downloads);
        }
        if let Some(site_ids) = input.site_ids {
            active.site_ids = Set(site_ids.map(|ids| serde_json::to_value(ids).unwrap_or_default()));
        }
        if let Some(download_client_id) = input.download_client_id {
            active.download_client_id = Set(download_client_id.and_then(|id| id.parse::<Uuid>().ok()));
        }

        let updated = active.update(db).await?;
        let fids = collect_filter_ids(&updated);
        let fnames = Self::resolve_filter_names(db, &fids).await;
        Ok(Some(to_dto(
            &updated,
            if fnames.is_empty() { None } else { Some(fnames) },
            None,
        )))
    }

    pub async fn delete<C: ConnectionTrait>(db: &C, id: &str) -> Result<bool, AppError> {
        let uid: Uuid = id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid subscription id".into()))?;
        let result = subscriptions::Entity::delete_by_id(uid).exec(db).await?;
        Ok(result.rows_affected > 0)
    }

    pub async fn update_timestamps<C: ConnectionTrait>(
        db: &C,
        id: &str,
        interval_minutes: i32,
    ) -> Result<(), AppError> {
        let uid: Uuid = id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid subscription id".into()))?;

        let Some(existing) = subscriptions::Entity::find_by_id(uid).one(db).await? else {
            return Err(AppError::NotFound("subscription not found".into()));
        };

        let now = Utc::now().fixed_offset();
        let next = now + chrono::Duration::minutes(i64::from(interval_minutes));

        let mut active: subscriptions::ActiveModel = existing.into();
        active.last_checked_at = Set(Some(now));
        active.next_check_at = Set(Some(next));
        active.updated_at = Set(Some(now));
        active.update(db).await?;
        Ok(())
    }

    /// Get episode progress from `download_records` via JSON metadata query.
    /// Uses raw SQL since the sidecar schema stores subscription_id in `app_metadata` JSONB.
    pub async fn get_episode_progress<C: ConnectionTrait>(
        db: &C,
        subscription_id: &str,
    ) -> Result<EpisodeProgress, AppError> {
        // Query app_metadata for records associated with this subscription
        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"SELECT app_metadata FROM download_records
               WHERE app_metadata->>'subscriptionId' = $1
                 AND status != 'failed'"#,
            [subscription_id.into()],
        );

        #[derive(Debug, sea_orm::FromQueryResult)]
        struct Row {
            app_metadata: Option<serde_json::Value>,
        }

        let rows = Row::find_by_statement(stmt).all(db).await.map_err(|e| {
            error!("get_episode_progress query failed: {e}");
            AppError::Database(e)
        })?;

        let mut downloaded = std::collections::BTreeSet::new();
        for row in &rows {
            let app_meta = row.app_metadata.as_ref().and_then(serde_json::Value::as_object);
            let episodes = app_meta.and_then(|obj| obj.get("episodes")).cloned();
            let episode = app_meta
                .and_then(|obj| obj.get("episode"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);

            if let Some(eps) = episodes.as_ref().and_then(|v| serde_json::from_value::<Vec<i32>>(v.clone()).ok()) {
                for ep in eps {
                    downloaded.insert(ep);
                }
            } else if let Some(ep_str) = episode
                && let Ok(ep) = ep_str.parse::<i32>()
            {
                downloaded.insert(ep);
            }
        }

        Ok(EpisodeProgress {
            downloaded_episodes: downloaded.into_iter().collect(),
            total_episodes: None,
        })
    }
}
