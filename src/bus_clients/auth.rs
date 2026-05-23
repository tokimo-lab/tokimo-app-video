//! Auth bus client — validates sessions & stream tokens via the main server's
//! `auth` virtual service.  Results are cached with short TTLs to minimise
//! round-trips on every media request.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use moka::Expiry;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use tokimo_bus_client::BusClient;
use uuid::Uuid;

use super::downloader::video_caller;

// ── TTLs ──────────────────────────────────────────────────────────────────────

const SESSION_TTL: Duration = Duration::from_secs(5);
const TOKEN_TTL: Duration = Duration::from_secs(5);
const USER_DISPLAY_TTL: Duration = Duration::from_secs(300); // 5 min
const NEGATIVE_TTL: Duration = Duration::from_secs(2);

// ── DTOs (mirrors the main-server auth service payloads) ─────────────────────

#[derive(Debug, Serialize)]
struct ValidateSessionReq<'a> {
    session_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct ValidateSessionResp {
    user_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
struct ValidateTokenReq<'a> {
    token: &'a str,
}

#[derive(Debug, Deserialize)]
struct ValidateTokenResp {
    valid: bool,
}

#[derive(Debug, Serialize)]
struct GetUserDisplayReq {
    user_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UserDisplayEntry {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct GetUserDisplayResp {
    users: Vec<UserDisplayEntry>,
}

// ── Per-entry expiry policies ─────────────────────────────────────────────────

struct SessionExpiry;
struct TokenExpiry;
struct UserDisplayExpiry;

impl Expiry<String, Option<Uuid>> for SessionExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &Option<Uuid>,
        _created_at: std::time::Instant,
    ) -> Option<Duration> {
        Some(if value.is_some() { SESSION_TTL } else { NEGATIVE_TTL })
    }
}

impl Expiry<String, bool> for TokenExpiry {
    fn expire_after_create(&self, _key: &String, value: &bool, _created_at: std::time::Instant) -> Option<Duration> {
        Some(if *value { TOKEN_TTL } else { NEGATIVE_TTL })
    }
}

impl Expiry<Uuid, Option<UserDisplayEntry>> for UserDisplayExpiry {
    fn expire_after_create(
        &self,
        _key: &Uuid,
        value: &Option<UserDisplayEntry>,
        _created_at: std::time::Instant,
    ) -> Option<Duration> {
        Some(if value.is_some() {
            USER_DISPLAY_TTL
        } else {
            NEGATIVE_TTL
        })
    }
}

// ── AuthClient ────────────────────────────────────────────────────────────────

pub struct AuthClient {
    bus_slot: Arc<OnceLock<Arc<BusClient>>>,
    // Bounded caches with per-entry TTL (pos vs neg) via moka Expiry.
    // get_with provides singleflight: concurrent misses on the same key
    // coalesce into a single bus invocation.
    session_cache: Cache<String, Option<Uuid>>,
    token_cache: Cache<String, bool>,
    user_display_cache: Cache<Uuid, Option<UserDisplayEntry>>,
}

impl AuthClient {
    pub fn new(bus_slot: Arc<OnceLock<Arc<BusClient>>>) -> Self {
        let session_cache = Cache::builder()
            .max_capacity(10_000)
            .expire_after(SessionExpiry)
            .build();
        let token_cache = Cache::builder().max_capacity(10_000).expire_after(TokenExpiry).build();
        let user_display_cache = Cache::builder()
            .max_capacity(50_000)
            .expire_after(UserDisplayExpiry)
            .build();
        Self {
            bus_slot,
            session_cache,
            token_cache,
            user_display_cache,
        }
    }

    fn client(&self) -> Option<Arc<BusClient>> {
        self.bus_slot.get().map(Arc::clone)
    }

    /// Validate a `SESSION_ID` cookie value.  Returns the `user_id` when the
    /// session exists and has not expired; `None` otherwise.
    pub async fn validate_session(&self, session_id: &str) -> Option<Uuid> {
        let key = session_id.to_string();
        let client_opt = self.client();
        self.session_cache
            .get_with(key.clone(), async move {
                let Some(client) = client_opt else { return None };
                let payload = serde_json::to_vec(&ValidateSessionReq { session_id: &key })
                    .map_err(|e| tracing::error!("auth: serialize validate_session: {e}"))
                    .ok()?;
                match client.invoke("auth", "validate_session", payload, video_caller()).await {
                    Ok(resp_bytes) => match serde_json::from_slice::<ValidateSessionResp>(&resp_bytes) {
                        Ok(r) => r.user_id,
                        Err(e) => {
                            tracing::error!("auth: deserialize validate_session resp: {e}");
                            None
                        }
                    },
                    Err(e) => {
                        tracing::error!("auth: validate_session bus error: {e}");
                        None
                    }
                }
            })
            .await
    }

    /// Validate an internal stream access token.
    pub async fn validate_internal_stream_token(&self, token: &str) -> bool {
        let key = token.to_string();
        let client_opt = self.client();
        self.token_cache
            .get_with(key.clone(), async move {
                let Some(client) = client_opt else { return false };
                let payload = match serde_json::to_vec(&ValidateTokenReq { token: &key }) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("auth: serialize validate_token: {e}");
                        return false;
                    }
                };
                match client
                    .invoke("auth", "validate_internal_stream_token", payload, video_caller())
                    .await
                {
                    Ok(resp_bytes) => match serde_json::from_slice::<ValidateTokenResp>(&resp_bytes) {
                        Ok(r) => r.valid,
                        Err(e) => {
                            tracing::error!("auth: deserialize validate_token resp: {e}");
                            false
                        }
                    },
                    Err(e) => {
                        tracing::error!("auth: validate_token bus error: {e}");
                        false
                    }
                }
            })
            .await
    }

    /// Get user display name for a single user, or `None` on failure.
    pub async fn get_user_name(&self, user_id: Uuid) -> Option<String> {
        let client_opt = self.client();
        self.user_display_cache
            .get_with(user_id, async move {
                let Some(client) = client_opt else { return None };
                let payload = serde_json::to_vec(&GetUserDisplayReq {
                    user_ids: vec![user_id],
                })
                .map_err(|e| tracing::error!("auth: serialize get_user_display: {e}"))
                .ok()?;
                match client.invoke("auth", "get_user_display", payload, video_caller()).await {
                    Ok(resp_bytes) => match serde_json::from_slice::<GetUserDisplayResp>(&resp_bytes) {
                        Ok(r) => r.users.into_iter().find(|e| e.id == user_id),
                        Err(e) => {
                            tracing::error!("auth: deserialize get_user_display resp: {e}");
                            None
                        }
                    },
                    Err(e) => {
                        tracing::error!("auth: get_user_display bus error: {e}");
                        None
                    }
                }
            })
            .await
            .map(|e| e.name)
    }
}
