// TODO(F7): remove this legacy system_config repo after scrape_settings migration.
use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;

use crate::db::entities::system_config;
use crate::error::AppError;

// ── SystemConfigSection trait ────────────────────────────────────────────────

/// Every typed config section implements this trait.
/// To add a new settings group: define a struct + `impl SystemConfigSection`.
/// No schema migration, no entity, no extra repo method needed.
pub trait SystemConfigSection: Serialize + DeserializeOwned + Send + Sync {
    /// Scope group (e.g. "video", "metadata", "download").
    const SCOPE: &'static str;
    /// Unique ID within the scope.
    const SCOPE_ID: &'static str;
    /// Value returned when no row exists yet.
    fn default_value() -> Self;
}

// ── SystemConfigRepo ─────────────────────────────────────────────────────────

#[deprecated(
    note = "F7 migrates scraping settings to video.scrape_settings; remove this repo after remaining system_config users move away"
)]
pub struct SystemConfigRepo;

impl SystemConfigRepo {
    // ── Typed access (requires SystemConfigSection) ──────────────────────────

    /// Get a typed config section (returns default if not stored yet).
    pub async fn get<T: SystemConfigSection>(db: &impl ConnectionTrait) -> Result<T, AppError> {
        let row = system_config::Entity::find_by_id((T::SCOPE.to_string(), T::SCOPE_ID.to_string()))
            .one(db)
            .await?;
        match row {
            Some(m) => Ok(serde_json::from_value(m.value)?),
            None => Ok(T::default_value()),
        }
    }

    /// Get a typed config section, returning `None` when the key doesn't exist.
    pub async fn get_optional<T: SystemConfigSection>(db: &impl ConnectionTrait) -> Result<Option<T>, AppError> {
        let row = system_config::Entity::find_by_id((T::SCOPE.to_string(), T::SCOPE_ID.to_string()))
            .one(db)
            .await?;
        match row {
            Some(m) => Ok(Some(serde_json::from_value(m.value)?)),
            None => Ok(None),
        }
    }

    /// Write (upsert) a typed config section.
    pub async fn set<T: SystemConfigSection>(db: &impl ConnectionTrait, value: &T) -> Result<(), AppError> {
        let json = serde_json::to_value(value)?;
        Self::set_raw(db, T::SCOPE, T::SCOPE_ID, json).await
    }

    // ── Raw access (dynamic scope/scope_id) ──────────────────────────────────

    /// Get raw JSONB value by (scope, scope_id).
    pub async fn get_raw(
        db: &impl ConnectionTrait,
        scope: &str,
        scope_id: &str,
    ) -> Result<Option<serde_json::Value>, AppError> {
        let row = system_config::Entity::find_by_id((scope.to_string(), scope_id.to_string()))
            .one(db)
            .await?;
        Ok(row.map(|r| r.value))
    }

    /// Get raw value and deserialize to T.
    pub async fn get_raw_as<T: DeserializeOwned>(
        db: &impl ConnectionTrait,
        scope: &str,
        scope_id: &str,
    ) -> Result<Option<T>, AppError> {
        match Self::get_raw(db, scope, scope_id).await? {
            Some(v) => Ok(Some(serde_json::from_value(v)?)),
            None => Ok(None),
        }
    }

    /// Upsert a raw (scope, scope_id, value) entry.
    pub async fn set_raw(
        db: &impl ConnectionTrait,
        scope: &str,
        scope_id: &str,
        value: serde_json::Value,
    ) -> Result<(), AppError> {
        let now = Utc::now().fixed_offset();
        system_config::Entity::insert(system_config::ActiveModel {
            scope: Set(scope.to_string()),
            scope_id: Set(scope_id.to_string()),
            value: Set(value),
            updated_at: Set(now),
        })
        .on_conflict(
            OnConflict::columns([system_config::Column::Scope, system_config::Column::ScopeId])
                .update_columns([system_config::Column::Value, system_config::Column::UpdatedAt])
                .to_owned(),
        )
        .exec(db)
        .await?;
        Ok(())
    }

    /// List all entries for a given scope, ordered by scope_id.
    /// Returns `Vec<(scope_id, value)>`.
    pub async fn list_scope(
        db: &impl ConnectionTrait,
        scope: &str,
    ) -> Result<Vec<(String, serde_json::Value)>, AppError> {
        let rows = system_config::Entity::find()
            .filter(system_config::Column::Scope.eq(scope))
            .order_by_asc(system_config::Column::ScopeId)
            .all(db)
            .await?;
        Ok(rows.into_iter().map(|r| (r.scope_id, r.value)).collect())
    }

    /// Delete a specific (scope, scope_id) entry.
    pub async fn delete(db: &impl ConnectionTrait, scope: &str, scope_id: &str) -> Result<(), AppError> {
        system_config::Entity::delete_by_id((scope.to_string(), scope_id.to_string()))
            .exec(db)
            .await?;
        Ok(())
    }

    /// Batch-read multiple (scope, scope_id) pairs in one SELECT.
    /// Returns a map keyed by `"scope/scope_id"`.
    pub async fn get_many(
        db: &impl ConnectionTrait,
        keys: &[(&str, &str)],
    ) -> Result<HashMap<String, serde_json::Value>, AppError> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let scope_ids: Vec<String> = keys.iter().map(|(_, id)| (*id).to_string()).collect();
        let scope_vals: Vec<String> = keys.iter().map(|(s, _)| (*s).to_string()).collect();

        let rows = system_config::Entity::find()
            .filter(system_config::Column::ScopeId.is_in(scope_ids))
            .filter(system_config::Column::Scope.is_in(scope_vals))
            .all(db)
            .await?;

        let map: HashMap<String, serde_json::Value> = rows
            .into_iter()
            .map(|r| (format!("{}/{}", r.scope, r.scope_id), r.value))
            .collect();
        Ok(map)
    }
}
