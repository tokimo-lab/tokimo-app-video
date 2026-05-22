use crate::db::OptionalApiDateTimeExt;
use chrono::Utc;
use sea_orm::{sea_query::Expr, *};
use serde::{Deserialize, Serialize};
use tracing::error;
use uuid::Uuid;

use crate::db::entities::subscription_filters;
use crate::error::{AppError, OptionExt};

// ── DTO ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionFilterDto {
    pub id: String,
    pub name: String,
    pub sources: Option<Vec<String>>,
    pub resolutions: Option<Vec<String>>,
    pub codecs: Option<Vec<String>>,
    pub release_groups: Option<Vec<String>>,
    pub min_size: f64,
    pub max_size: f64,
    pub min_seeders: f64,
    pub max_seeders: f64,
    pub include_keywords: Option<String>,
    pub exclude_keywords: Option<String>,
    pub free_only: bool,
    pub exclude_hr: bool,
    pub sort_order: i32,
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn json_to_string_vec(val: &Option<serde_json::Value>) -> Option<Vec<String>> {
    val.as_ref().and_then(|v| {
        v.as_array()
            .map(|arr| arr.iter().filter_map(|item| item.as_str().map(String::from)).collect())
    })
}

fn string_vec_to_json(val: &Option<Vec<String>>) -> Option<serde_json::Value> {
    val.as_ref()
        .map(|v| serde_json::Value::Array(v.iter().map(|s| serde_json::Value::String(s.clone())).collect()))
}

fn parse_num(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(0.0)
}

fn to_dto(m: subscription_filters::Model, creator_name: Option<String>) -> SubscriptionFilterDto {
    SubscriptionFilterDto {
        id: m.id.to_string(),
        name: m.name,
        sources: json_to_string_vec(&m.sources),
        resolutions: json_to_string_vec(&m.resolutions),
        codecs: json_to_string_vec(&m.codecs),
        release_groups: json_to_string_vec(&m.release_groups),
        min_size: parse_num(&m.min_size),
        max_size: parse_num(&m.max_size),
        min_seeders: parse_num(&m.min_seeders),
        max_seeders: parse_num(&m.max_seeders),
        include_keywords: m.include_keywords,
        exclude_keywords: m.exclude_keywords,
        free_only: m.free_only,
        exclude_hr: m.exclude_hr,
        sort_order: m.sort_order,
        created_by: m.created_by.map(|u| u.to_string()),
        created_by_name: creator_name,
        created_at: m.created_at.to_api_datetime_or_default(),
        updated_at: m.updated_at.to_api_datetime_or_default(),
    }
}

// ── Input structs ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSubscriptionFilterInput {
    pub name: String,
    pub sources: Option<Vec<String>>,
    pub resolutions: Option<Vec<String>>,
    pub codecs: Option<Vec<String>>,
    pub release_groups: Option<Vec<String>>,
    #[serde(default)]
    pub min_size: Option<f64>,
    #[serde(default)]
    pub max_size: Option<f64>,
    #[serde(default)]
    pub min_seeders: Option<f64>,
    #[serde(default)]
    pub max_seeders: Option<f64>,
    pub include_keywords: Option<String>,
    pub exclude_keywords: Option<String>,
    #[serde(default)]
    pub free_only: Option<bool>,
    #[serde(default)]
    pub exclude_hr: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubscriptionFilterInput {
    pub name: Option<String>,
    pub sources: Option<Option<Vec<String>>>,
    pub resolutions: Option<Option<Vec<String>>>,
    pub codecs: Option<Option<Vec<String>>>,
    pub release_groups: Option<Option<Vec<String>>>,
    pub min_size: Option<f64>,
    pub max_size: Option<f64>,
    pub min_seeders: Option<f64>,
    pub max_seeders: Option<f64>,
    pub include_keywords: Option<Option<String>>,
    pub exclude_keywords: Option<Option<String>>,
    pub free_only: Option<bool>,
    pub exclude_hr: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderItem {
    pub id: String,
    pub sort_order: i32,
}

// ── Repo ────────────────────────────────────────────────────────────────────

pub struct SubscriptionFilterRepo;

impl SubscriptionFilterRepo {
    pub async fn list<C: ConnectionTrait>(
        db: &C,
        user_id: &str,
    ) -> Result<Vec<SubscriptionFilterDto>, AppError> {
        let uid: Uuid = user_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid user id".into()))?;

        let rows = subscription_filters::Entity::find()
            .filter(subscription_filters::Column::CreatedBy.eq(uid))
            .order_by_asc(subscription_filters::Column::SortOrder)
            .order_by_asc(subscription_filters::Column::CreatedAt)
            .all(db)
            .await
            .map_err(|e| {
                error!("subscription_filter list failed: {e}");
                AppError::Database(e)
            })?;

        Ok(rows
            .into_iter()
            .map(|filter| to_dto(filter, None))
            .collect())
    }

    pub async fn get_by_id<C: ConnectionTrait>(
        db: &C,
        id: &str,
    ) -> Result<Option<SubscriptionFilterDto>, AppError> {
        let uid: Uuid = id.parse().map_err(|_| AppError::BadRequest("invalid id".into()))?;

        let row = subscription_filters::Entity::find_by_id(uid)
            .one(db)
            .await
            .map_err(|e| {
                error!("subscription_filter get_by_id failed: {e}");
                AppError::Database(e)
            })?;

        Ok(row.map(|filter| to_dto(filter, None)))
    }

    pub async fn get_raw_by_id<C: ConnectionTrait>(
        db: &C,
        id: &str,
    ) -> Result<Option<subscription_filters::Model>, AppError> {
        let uid: Uuid = id.parse().map_err(|_| AppError::BadRequest("invalid id".into()))?;

        subscription_filters::Entity::find_by_id(uid)
            .one(db)
            .await
            .map_err(|e| {
                error!("subscription_filter get_raw_by_id failed: {e}");
                AppError::Database(e)
            })
    }

    pub async fn create<C: ConnectionTrait>(
        db: &C,
        input: CreateSubscriptionFilterInput,
        user_id: &str,
    ) -> Result<SubscriptionFilterDto, AppError> {
        let uid: Uuid = user_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid user id".into()))?;

        let now = Utc::now().fixed_offset();
        let model = subscription_filters::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(input.name),
            sources: Set(string_vec_to_json(&input.sources)),
            resolutions: Set(string_vec_to_json(&input.resolutions)),
            codecs: Set(string_vec_to_json(&input.codecs)),
            release_groups: Set(string_vec_to_json(&input.release_groups)),
            min_size: Set(input.min_size.unwrap_or(0.0).to_string()),
            max_size: Set(input.max_size.unwrap_or(0.0).to_string()),
            min_seeders: Set(input.min_seeders.unwrap_or(0.0).to_string()),
            max_seeders: Set(input.max_seeders.unwrap_or(0.0).to_string()),
            include_keywords: Set(input.include_keywords),
            exclude_keywords: Set(input.exclude_keywords),
            free_only: Set(input.free_only.unwrap_or(false)),
            exclude_hr: Set(input.exclude_hr.unwrap_or(false)),
            created_by: Set(Some(uid)),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            sort_order: Set(0),
        };

        let created = subscription_filters::Entity::insert(model)
            .exec_with_returning(db)
            .await
            .map_err(|e| {
                error!("subscription_filter create failed: {e}");
                AppError::Database(e)
            })?;

        Self::get_by_id(db, &created.id.to_string())
            .await?
            .internal("failed to fetch created filter")
    }

    pub async fn update<C: ConnectionTrait>(
        db: &C,
        id: &str,
        input: UpdateSubscriptionFilterInput,
    ) -> Result<Option<SubscriptionFilterDto>, AppError> {
        let uid: Uuid = id.parse().map_err(|_| AppError::BadRequest("invalid id".into()))?;

        let existing = subscription_filters::Entity::find_by_id(uid)
            .one(db)
            .await
            .map_err(|e| {
                error!("subscription_filter update find failed: {e}");
                AppError::Database(e)
            })?;

        let Some(existing) = existing else {
            return Ok(None);
        };

        let mut active: subscription_filters::ActiveModel = existing.into();

        if let Some(name) = input.name {
            active.name = Set(name);
        }
        if let Some(sources) = input.sources {
            active.sources = Set(string_vec_to_json(&sources));
        }
        if let Some(resolutions) = input.resolutions {
            active.resolutions = Set(string_vec_to_json(&resolutions));
        }
        if let Some(codecs) = input.codecs {
            active.codecs = Set(string_vec_to_json(&codecs));
        }
        if let Some(release_groups) = input.release_groups {
            active.release_groups = Set(string_vec_to_json(&release_groups));
        }
        if let Some(min_size) = input.min_size {
            active.min_size = Set(min_size.to_string());
        }
        if let Some(max_size) = input.max_size {
            active.max_size = Set(max_size.to_string());
        }
        if let Some(min_seeders) = input.min_seeders {
            active.min_seeders = Set(min_seeders.to_string());
        }
        if let Some(max_seeders) = input.max_seeders {
            active.max_seeders = Set(max_seeders.to_string());
        }
        if let Some(include_keywords) = input.include_keywords {
            active.include_keywords = Set(include_keywords);
        }
        if let Some(exclude_keywords) = input.exclude_keywords {
            active.exclude_keywords = Set(exclude_keywords);
        }
        if let Some(free_only) = input.free_only {
            active.free_only = Set(free_only);
        }
        if let Some(exclude_hr) = input.exclude_hr {
            active.exclude_hr = Set(exclude_hr);
        }

        active.updated_at = Set(Some(Utc::now().fixed_offset()));

        active.update(db).await.map_err(|e| {
            error!("subscription_filter update failed: {e}");
            AppError::Database(e)
        })?;

        Self::get_by_id(db, id).await
    }

    pub async fn delete<C: ConnectionTrait>(db: &C, id: &str) -> Result<bool, AppError> {
        let uid: Uuid = id.parse().map_err(|_| AppError::BadRequest("invalid id".into()))?;

        let result = subscription_filters::Entity::delete_by_id(uid)
            .exec(db)
            .await
            .map_err(|e| {
                error!("subscription_filter delete failed: {e}");
                AppError::Database(e)
            })?;

        Ok(result.rows_affected > 0)
    }

    pub async fn reorder<C: ConnectionTrait>(db: &C, orders: Vec<ReorderItem>) -> Result<(), AppError> {
        for item in orders {
            let uid: Uuid = item
                .id
                .parse()
                .map_err(|_| AppError::BadRequest("invalid id in reorder".into()))?;

            subscription_filters::Entity::update_many()
                .filter(subscription_filters::Column::Id.eq(uid))
                .col_expr(subscription_filters::Column::SortOrder, Expr::value(item.sort_order))
                .exec(db)
                .await
                .map_err(|e| {
                    error!("subscription_filter reorder update failed: {e}");
                    AppError::Database(e)
                })?;
        }
        Ok(())
    }
}
