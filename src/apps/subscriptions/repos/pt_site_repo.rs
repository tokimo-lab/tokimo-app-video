use crate::db::{ApiDateTimeExt, OptionalApiDateTimeExt};
use chrono::Utc;
use sea_orm::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::db::entities::pt_sites;
use crate::apps::subscriptions::models::pt_site::PtSiteDto;
use crate::error::{AppError, OptionExt};

// ── Conversion ────────────────────────────────────────────────────────────────

pub fn to_dto(m: pt_sites::Model) -> PtSiteDto {
    PtSiteDto {
        id: m.id.to_string(),
        name: m.name,
        site_id: m.site_id,
        domain: m.domain,
        auth_type: m.auth_type,
        cookies: m.cookies,
        api_key: m.api_key,
        auto_stop_minutes: m.auto_stop_minutes.as_deref().and_then(|s| s.parse::<i64>().ok()),
        traffic_manage_enabled: m.traffic_manage_enabled,
        traffic_manage_mode: m.traffic_manage_mode,
        traffic_manage_target: m.traffic_manage_target,
        adult_enabled: m.adult_enabled,
        sort_order: m.sort_order,
        last_checked_at: m.last_checked_at.to_api_datetime(),
        created_at: m.created_at.to_api_datetime_or_default(),
        updated_at: m.updated_at.to_api_datetime_or_default(),
    }
}

// ── Input types ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePtSiteInput {
    pub name: String,
    pub site_id: String,
    pub domain: Option<String>,
    pub auth_type: Option<String>,
    pub cookies: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub auto_stop_minutes: Option<i64>,
    pub adult_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePtSiteInput {
    pub site_id: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub domain: Option<Option<String>>,
    pub auth_type: Option<String>,
    #[serde(default)]
    pub cookies: Option<Option<String>>,
    #[serde(default)]
    pub api_key: Option<Option<String>>,
    #[serde(default)]
    pub auto_stop_minutes: Option<Option<i64>>,
    pub traffic_manage_enabled: Option<bool>,
    pub traffic_manage_mode: Option<String>,
    #[serde(default)]
    pub traffic_manage_target: Option<Option<String>>,
    pub adult_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderItem {
    pub id: String,
    pub sort_order: i32,
}

// ── Repo ──────────────────────────────────────────────────────────────────────

pub struct PtSiteRepo;

impl PtSiteRepo {
    pub async fn list<C: ConnectionTrait>(db: &C) -> Result<Vec<PtSiteDto>, AppError> {
        let sites = pt_sites::Entity::find()
            .order_by_asc(pt_sites::Column::SortOrder)
            .order_by_asc(pt_sites::Column::CreatedAt)
            .all(db)
            .await?;
        Ok(sites.into_iter().map(to_dto).collect())
    }

    pub async fn get_by_id<C: ConnectionTrait>(db: &C, id: &str) -> Result<Option<PtSiteDto>, AppError> {
        let uid = Uuid::parse_str(id).map_err(|_| AppError::BadRequest("无效的 ID".into()))?;
        let site = pt_sites::Entity::find_by_id(uid).one(db).await?;
        Ok(site.map(to_dto))
    }

    pub async fn get_by_site_id<C: ConnectionTrait>(
        db: &C,
        site_id: &str,
    ) -> Result<Option<PtSiteDto>, AppError> {
        let site = pt_sites::Entity::find()
            .filter(pt_sites::Column::SiteId.eq(site_id))
            .one(db)
            .await?;
        Ok(site.map(to_dto))
    }

    pub async fn create<C: ConnectionTrait>(
        db: &C,
        input: CreatePtSiteInput,
        resolved_domain: &str,
    ) -> Result<PtSiteDto, AppError> {
        let now = Utc::now().fixed_offset();
        let active = pt_sites::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(input.name),
            site_id: Set(input.site_id),
            domain: Set(resolved_domain.to_string()),
            auth_type: Set(input.auth_type.unwrap_or_else(|| "cookies".to_string())),
            cookies: Set(input.cookies),
            api_key: Set(input.api_key),
            config_yaml: Set(None),
            config_url: Set(None),
            auto_stop_minutes: Set(input.auto_stop_minutes.map(|m| m.to_string())),
            traffic_manage_enabled: Set(false),
            traffic_manage_mode: Set("active".to_string()),
            traffic_manage_target: Set(None),
            adult_enabled: Set(input.adult_enabled.unwrap_or(false)),
            sort_order: Set(0),
            last_checked_at: Set(None),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
        };
        let inserted = active.insert(db).await?;
        Ok(to_dto(inserted))
    }

    pub async fn update<C: ConnectionTrait>(
        db: &C,
        id: &str,
        input: UpdatePtSiteInput,
    ) -> Result<PtSiteDto, AppError> {
        let uid = Uuid::parse_str(id).map_err(|_| AppError::BadRequest("无效的 ID".into()))?;
        let model = pt_sites::Entity::find_by_id(uid)
            .one(db)
            .await?
            .not_found("PT 站点不存在")?;

        let now = Utc::now().fixed_offset();
        let mut active: pt_sites::ActiveModel = model.into();

        if let Some(v) = input.site_id {
            active.site_id = Set(v);
        }
        if let Some(v) = input.name {
            active.name = Set(v);
        }
        if let Some(v) = input.domain
            && let Some(d) = v
            && !d.is_empty()
        {
            active.domain = Set(d);
        }
        if let Some(v) = input.auth_type {
            active.auth_type = Set(v);
        }
        if let Some(v) = input.cookies {
            active.cookies = Set(v);
        }
        if let Some(v) = input.api_key {
            active.api_key = Set(v);
        }
        if let Some(v) = input.auto_stop_minutes {
            active.auto_stop_minutes = Set(v.map(|m| m.to_string()));
        }
        if let Some(v) = input.traffic_manage_enabled {
            active.traffic_manage_enabled = Set(v);
        }
        if let Some(v) = input.traffic_manage_mode {
            active.traffic_manage_mode = Set(v);
        }
        if let Some(v) = input.traffic_manage_target {
            active.traffic_manage_target = Set(v);
        }
        if let Some(v) = input.adult_enabled {
            active.adult_enabled = Set(v);
        }
        active.updated_at = Set(Some(now));

        let updated = active.update(db).await?;
        Ok(to_dto(updated))
    }

    pub async fn delete<C: ConnectionTrait>(db: &C, id: &str) -> Result<(), AppError> {
        let uid = Uuid::parse_str(id).map_err(|_| AppError::BadRequest("无效的 ID".into()))?;
        let result = pt_sites::Entity::delete_by_id(uid).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound("PT 站点不存在".into()));
        }
        Ok(())
    }

    pub async fn reorder<C: ConnectionTrait>(db: &C, items: Vec<ReorderItem>) -> Result<(), AppError> {
        for item in items {
            let uid = Uuid::parse_str(&item.id).map_err(|_| AppError::BadRequest("无效的 ID".into()))?;
            if let Some(model) = pt_sites::Entity::find_by_id(uid).one(db).await? {
                let mut active: pt_sites::ActiveModel = model.into();
                active.sort_order = Set(item.sort_order);
                active.update(db).await?;
            }
        }
        Ok(())
    }

    pub async fn update_last_checked<C: ConnectionTrait>(db: &C, id: &str) -> Result<(), AppError> {
        let uid = Uuid::parse_str(id).map_err(|_| AppError::BadRequest("无效的 ID".into()))?;
        let model = pt_sites::Entity::find_by_id(uid)
            .one(db)
            .await?
            .not_found("PT 站点不存在")?;

        let now = Utc::now().fixed_offset();
        let mut active: pt_sites::ActiveModel = model.into();
        active.last_checked_at = Set(Some(now));
        active.updated_at = Set(Some(now));
        active.update(db).await?;
        Ok(())
    }
}
