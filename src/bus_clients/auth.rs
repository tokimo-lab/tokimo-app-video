//! Auth bus client — validates sessions & stream tokens via the main server's
//! `auth` virtual service.  Results are cached with short TTLs to minimise
//! round-trips on every media request.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokimo_bus_client::BusClient;
use uuid::Uuid;

use super::downloader::video_caller;

// ── TTLs ──────────────────────────────────────────────────────────────────────

const SESSION_TTL: Duration = Duration::from_secs(30);
const TOKEN_TTL: Duration = Duration::from_secs(30);
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
    pub email: String,
}

#[derive(Debug, Deserialize)]
struct GetUserDisplayResp {
    users: Vec<UserDisplayEntry>,
}

// ── AuthClient ────────────────────────────────────────────────────────────────

pub struct AuthClient {
    bus_slot: Arc<OnceLock<Arc<BusClient>>>,
    // key=session_id, value=(user_id, cached_at)
    session_cache: DashMap<String, (Option<Uuid>, Instant)>,
    // key=token, value=(valid, cached_at)
    token_cache: DashMap<String, (bool, Instant)>,
    // key=user_id, value=(entry, cached_at)
    user_display_cache: DashMap<Uuid, (UserDisplayEntry, Instant)>,
}

impl AuthClient {
    pub fn new(bus_slot: Arc<OnceLock<Arc<BusClient>>>) -> Self {
        Self {
            bus_slot,
            session_cache: DashMap::new(),
            token_cache: DashMap::new(),
            user_display_cache: DashMap::new(),
        }
    }

    fn client(&self) -> Option<Arc<BusClient>> {
        self.bus_slot.get().map(Arc::clone)
    }

    /// Validate a `SESSION_ID` cookie value.  Returns the `user_id` when the
    /// session exists and has not expired; `None` otherwise.
    pub async fn validate_session(&self, session_id: &str) -> Option<Uuid> {
        // check cache
        if let Some(entry) = self.session_cache.get(session_id) {
            let (user_id, cached_at) = entry.value().clone();
            let ttl = if user_id.is_some() { SESSION_TTL } else { NEGATIVE_TTL };
            if cached_at.elapsed() < ttl {
                return user_id;
            }
        }

        let client = self.client()?;
        let payload = serde_json::to_vec(&ValidateSessionReq { session_id })
            .map_err(|e| tracing::error!("auth: serialize validate_session: {e}"))
            .ok()?;

        match client
            .invoke("auth", "validate_session", payload, video_caller())
            .await
        {
            Ok(resp_bytes) => {
                let resp: ValidateSessionResp = match serde_json::from_slice(&resp_bytes) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("auth: deserialize validate_session resp: {e}");
                        return None;
                    }
                };
                self.session_cache
                    .insert(session_id.to_string(), (resp.user_id, Instant::now()));
                resp.user_id
            }
            Err(e) => {
                tracing::error!("auth: validate_session bus error: {e}");
                None
            }
        }
    }

    /// Validate an internal stream access token.
    pub async fn validate_internal_stream_token(&self, token: &str) -> bool {
        if let Some(entry) = self.token_cache.get(token) {
            let (valid, cached_at) = *entry.value();
            let ttl = if valid { TOKEN_TTL } else { NEGATIVE_TTL };
            if cached_at.elapsed() < ttl {
                return valid;
            }
        }

        let Some(client) = self.client() else { return false };
        let payload = match serde_json::to_vec(&ValidateTokenReq { token }) {
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
            Ok(resp_bytes) => {
                let resp: ValidateTokenResp = match serde_json::from_slice(&resp_bytes) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("auth: deserialize validate_token resp: {e}");
                        return false;
                    }
                };
                self.token_cache
                    .insert(token.to_string(), (resp.valid, Instant::now()));
                resp.valid
            }
            Err(e) => {
                tracing::error!("auth: validate_token bus error: {e}");
                false
            }
        }
    }

    /// Get user display name for a single user, or `None` on failure.
    pub async fn get_user_name(&self, user_id: Uuid) -> Option<String> {
        // warm from cache
        if let Some(entry) = self.user_display_cache.get(&user_id) {
            let (display, cached_at) = entry.value().clone();
            if cached_at.elapsed() < USER_DISPLAY_TTL {
                return Some(display.name);
            }
        }

        let client = self.client()?;
        let payload = serde_json::to_vec(&GetUserDisplayReq {
            user_ids: vec![user_id],
        })
        .map_err(|e| tracing::error!("auth: serialize get_user_display: {e}"))
        .ok()?;

        match client
            .invoke("auth", "get_user_display", payload, video_caller())
            .await
        {
            Ok(resp_bytes) => {
                let resp: GetUserDisplayResp = match serde_json::from_slice(&resp_bytes) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("auth: deserialize get_user_display resp: {e}");
                        return None;
                    }
                };
                for entry in resp.users {
                    if entry.id == user_id {
                        let name = entry.name.clone();
                        self.user_display_cache
                            .insert(entry.id, (entry, Instant::now()));
                        return Some(name);
                    }
                }
                None
            }
            Err(e) => {
                tracing::error!("auth: get_user_display bus error: {e}");
                None
            }
        }
    }
}
