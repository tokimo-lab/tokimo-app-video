use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use rust_online_media_ingest::provider_catalog;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AppState,
    db::repos::ytdlp_provider_auth_repo::YtdlpProviderAuthRepo,
    error::AppError,
    handlers::{ApiResponse, ok},
};

// ──────────────────────────────────────────────────────────────────────────────
// DTOs
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderListEntry {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub source_site: String,
    pub supported_content_types: Vec<String>,
    pub requires_auth: bool,
    pub auth_configurable: bool,
    pub common_source_sites: Vec<String>,
    pub source_site_aliases: Vec<String>,
    pub host_suffixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderListEntry>,
    pub ytdlp_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineMediaAuthData {
    pub display_name: String,
    pub cookie: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AuthSettingResponse {
    pub provider_id: String,
    pub display_name: String,
    pub requires_auth: bool,
    pub cookie: Option<String>,
    pub is_enabled: bool,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateAuthSettingRequest {
    pub display_name: Option<String>,
    pub cookie: Option<String>,
    pub is_enabled: Option<bool>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Handlers
// ──────────────────────────────────────────────────────────────────────────────

pub async fn list_providers() -> Result<Json<ApiResponse<ProvidersResponse>>, AppError> {
    let catalog_response = provider_catalog::list_all_providers_with_ytdlp().await;

    let providers = catalog_response
        .providers
        .into_iter()
        .map(|p| ProviderListEntry {
            id: p.id,
            name: p.name,
            display_name: p.display_name,
            source_site: p.source_site,
            supported_content_types: p.supported_content_types,
            requires_auth: p.requires_auth,
            auth_configurable: p.auth_configurable,
            common_source_sites: p.common_source_sites,
            source_site_aliases: p.source_site_aliases,
            host_suffixes: p.host_suffixes,
        })
        .collect();

    Ok(ok(ProvidersResponse {
        providers,
        ytdlp_available: catalog_response.ytdlp_available,
    }))
}

pub async fn get_auth_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<AuthSettingResponse>>>, AppError> {
    let db = &state.db;

    // Get all auth-configurable providers from catalog
    let all_providers = provider_catalog::list_all_providers();
    let auth_configurable_providers: Vec<_> = all_providers
        .iter()
        .filter(|p| p.auth_configurable)
        .collect();

    // Get stored settings
    let stored_settings = YtdlpProviderAuthRepo::get_all(db).await?;
    let stored_map: std::collections::HashMap<String, _> = stored_settings
        .into_iter()
        .map(|s| (s.provider.clone(), s))
        .collect();

    // Merge: for each auth-configurable provider, use stored value or defaults
    let mut results = Vec::new();
    for provider in auth_configurable_providers {
        let response = if let Some(stored) = stored_map.get(&provider.id) {
            // Parse stored value
            let auth_data: OnlineMediaAuthData = serde_json::from_value(stored.value.clone())
                .unwrap_or_else(|_| OnlineMediaAuthData {
                    display_name: provider.display_name.clone(),
                    cookie: None,
                    is_enabled: true,
                });

            AuthSettingResponse {
                provider_id: provider.id.clone(),
                display_name: auth_data.display_name,
                requires_auth: provider.requires_auth,
                cookie: auth_data.cookie,
                is_enabled: auth_data.is_enabled,
                updated_at: Some(stored.updated_at.to_rfc3339()),
            }
        } else {
            // Default for providers without stored settings
            AuthSettingResponse {
                provider_id: provider.id.clone(),
                display_name: provider.display_name.clone(),
                requires_auth: provider.requires_auth,
                cookie: None,
                is_enabled: true,
                updated_at: None,
            }
        };
        results.push(response);
    }

    Ok(ok(results))
}

pub async fn update_auth_setting(
    State(state): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    Json(req): Json<UpdateAuthSettingRequest>,
) -> Result<Json<ApiResponse<AuthSettingResponse>>, AppError> {
    let db = &state.db;

    // Validate provider exists and is auth-configurable
    let all_providers = provider_catalog::list_all_providers();
    let provider = all_providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| AppError::NotFound(format!("provider not found: {}", provider_id)))?;

    if !provider.auth_configurable {
        return Err(AppError::BadRequest(format!(
            "provider {} does not support auth configuration",
            provider_id
        )));
    }

    // Get current stored setting if it exists
    let current = YtdlpProviderAuthRepo::get_one(db, &provider_id).await?;
    let current_data: Option<OnlineMediaAuthData> = current
        .as_ref()
        .and_then(|s| serde_json::from_value(s.value.clone()).ok());

    // Build new auth data: merge request with current/catalog defaults
    let display_name = req
        .display_name
        .or_else(|| current_data.as_ref().map(|d| d.display_name.clone()))
        .unwrap_or_else(|| provider.display_name.clone());

    // Handle cookie: None or whitespace-only becomes None; otherwise preserve or use new value
    let cookie = match req.cookie {
        Some(c) => {
            let trimmed = c.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => current_data.as_ref().and_then(|d| d.cookie.clone()),
    };

    let is_enabled = req
        .is_enabled
        .or_else(|| current_data.as_ref().map(|d| d.is_enabled))
        .unwrap_or(true);

    let auth_data = OnlineMediaAuthData {
        display_name: display_name.clone(),
        cookie: cookie.clone(),
        is_enabled,
    };

    let value = serde_json::to_value(&auth_data)
        .map_err(|e| AppError::Internal(format!("failed to serialize auth data: {}", e)))?;

    // Upsert via repo
    let updated = YtdlpProviderAuthRepo::upsert(db, &provider_id, value).await?;

    Ok(ok(AuthSettingResponse {
        provider_id,
        display_name,
        requires_auth: provider.requires_auth,
        cookie,
        is_enabled,
        updated_at: Some(updated.updated_at.to_rfc3339()),
    }))
}
