//! `video_jobs` bus service — sidecar-side handler registration.
//!
//! Exposes six methods so the main server worker can hand off video jobs
//! via `client.invoke("video", "<method>", payload, caller)` instead of
//! running the handler inline.
//!
//! | bus method                  | inner handler                            |
//! |-----------------------------|------------------------------------------|
//! | `dispatch_file_scrape`      | `queue::handlers::file_scrape::handle`   |
//! | `dispatch_tv_scrape`        | `queue::tv_scrape::handle`               |
//! | `dispatch_movie_scrape`     | `queue::video_item_scrape::handle`       |
//! | `dispatch_tmdb_person_scrape` | `queue::tmdb_person_scrape::handle`    |
//! | `dispatch_online_video_download` | `queue::online_media_download::handle` |
//! | `capabilities`              | bus capability handshake                 |
//!
//! # Payload contract (JSON)
//! ```json
//! { "job": <JobOutput> }
//! ```
//! The handler receives `job.payload` as the `payload: &JsonValue` argument
//! and `job.id` as `job_id: Uuid`. A fresh `CancellationToken` (never
//! cancelled) is created per invocation — job lifecycle (claim / mark done /
//! mark failed) stays with the main server worker (C2b).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokimo_bus_client::BusClientBuilder;
use tokimo_bus_protocol::{BusError, HttpMethod, MethodDecl};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::AppState;

// ── helpers ───────────────────────────────────────────────────────────────────

fn decl(name: &str, description: &str) -> MethodDecl {
    MethodDecl {
        name: name.into(),
        description: Some(description.into()),
        requires_auth: false,
        streaming: false,
        http_method: HttpMethod::Post,
        path: None,
    }
}

#[derive(Deserialize)]
struct ImageProxySignInput {
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageProxySignOutput {
    proxy_path: String,
}

/// Extract user_id from CallerCtx (set by host's dispatch).
fn caller_user_id(caller: &tokimo_bus_protocol::CallerCtx) -> Option<Uuid> {
    caller.user_id.as_deref().and_then(|s| Uuid::parse_str(s).ok())
}

/// Decode `{ "job": { "id": "...", "payload": {...} } }` from JSON bytes.
fn decode_request(raw: &[u8]) -> Result<(Uuid, JsonValue), BusError> {
    let v: JsonValue = serde_json::from_slice(raw).map_err(|e| BusError::BadRequest(format!("json decode: {e}")))?;
    let job = v
        .get("job")
        .ok_or_else(|| BusError::BadRequest("missing 'job' field".into()))?;
    let job_id = job
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BusError::BadRequest("missing 'job.id'".into()))
        .and_then(|s| Uuid::parse_str(s).map_err(|e| BusError::BadRequest(format!("job.id UUID: {e}"))))?;
    let params = job.get("payload").cloned().unwrap_or(JsonValue::Null);
    Ok((job_id, params))
}

// ── registration ──────────────────────────────────────────────────────────────

/// Add all `video_jobs` method declarations and handlers to `builder`.
///
/// Called in `main.rs` before `.build()` so the methods are advertised to
/// the broker and the client can service inbound `Invoke` frames.
pub fn register(builder: BusClientBuilder, ctx: Arc<AppState>) -> BusClientBuilder {
    let ctx_file = ctx.clone();
    let ctx_tv = ctx.clone();
    let ctx_movie = ctx.clone();
    let ctx_person = ctx.clone();
    let ctx_download = ctx.clone();
    let ctx_proxy = ctx.clone();
    let ctx_ffprobe = ctx.clone();

    builder
        // ── dispatch_file_scrape ──────────────────────────────────────────────
        .method(decl(
            "dispatch_file_scrape",
            "Run a file_scrape job on behalf of the main worker",
        ))
        .on_invoke("dispatch_file_scrape", move |req| {
            let ctx = ctx_file.clone();
            async move {
                let (job_id, params) = decode_request(&req.payload)?;
                let user_id = caller_user_id(&req.caller);
                let cancel = CancellationToken::new();
                crate::queue::handlers::file_scrape::handle(&ctx.db, &ctx, job_id, &params, &cancel, user_id)
                    .await
                    .map(|_| b"{}".to_vec())
                    .map_err(|e| BusError::Internal(e.to_string()))
            }
        })
        // ── dispatch_tv_scrape ───────────────────────────────────────────────
        .method(decl(
            "dispatch_tv_scrape",
            "Run a tv_scrape job on behalf of the main worker",
        ))
        .on_invoke("dispatch_tv_scrape", move |req| {
            let ctx = ctx_tv.clone();
            async move {
                let (job_id, params) = decode_request(&req.payload)?;
                let user_id = caller_user_id(&req.caller);
                let cancel = CancellationToken::new();
                crate::queue::tv_scrape::handle(&ctx.db, &ctx, job_id, &params, &cancel, user_id)
                    .await
                    .map(|_| b"{}".to_vec())
                    .map_err(|e| BusError::Internal(e.to_string()))
            }
        })
        // ── dispatch_movie_scrape ────────────────────────────────────────────
        .method(decl(
            "dispatch_movie_scrape",
            "Run a movie_scrape (video_item_scrape) job on behalf of the main worker",
        ))
        .on_invoke("dispatch_movie_scrape", move |req| {
            let ctx = ctx_movie.clone();
            async move {
                let (job_id, params) = decode_request(&req.payload)?;
                let user_id = caller_user_id(&req.caller);
                let cancel = CancellationToken::new();
                crate::queue::video_item_scrape::handle(&ctx.db, &ctx, job_id, &params, &cancel, user_id)
                    .await
                    .map(|_| b"{}".to_vec())
                    .map_err(|e| BusError::Internal(e.to_string()))
            }
        })
        // ── dispatch_tmdb_person_scrape ──────────────────────────────────────
        .method(decl(
            "dispatch_tmdb_person_scrape",
            "Run a tmdb_person_scrape job on behalf of the main worker",
        ))
        .on_invoke("dispatch_tmdb_person_scrape", move |req| {
            let ctx = ctx_person.clone();
            async move {
                let (job_id, params) = decode_request(&req.payload)?;
                let user_id = caller_user_id(&req.caller);
                let cancel = CancellationToken::new();
                crate::queue::tmdb_person_scrape::handle(&ctx.db, &ctx, job_id, &params, &cancel, user_id)
                    .await
                    .map(|_| b"{}".to_vec())
                    .map_err(|e| BusError::Internal(e.to_string()))
            }
        })
        // ── dispatch_online_video_download ───────────────────────────────────
        .method(decl(
            "dispatch_online_video_download",
            "Run an online_media_ingest job on behalf of the main worker",
        ))
        .on_invoke("dispatch_online_video_download", move |req| {
            let ctx = ctx_download.clone();
            async move {
                let (job_id, params) = decode_request(&req.payload)?;
                let user_id = caller_user_id(&req.caller);
                let cancel = CancellationToken::new();
                crate::queue::online_media_download::handle(&ctx.db, &ctx, job_id, &params, &cancel, user_id)
                    .await
                    .map(|_| b"{}".to_vec())
                    .map_err(|e| BusError::Internal(e.to_string()))
            }
        })
        // ── dispatch_media_file_ffprobe ──────────────────────────────────────
        .method(decl(
            "dispatch_media_file_ffprobe",
            "Run ffprobe for a single media file on behalf of the main worker",
        ))
        .on_invoke("dispatch_media_file_ffprobe", move |req| {
            let ctx = ctx_ffprobe.clone();
            async move {
                #[derive(serde::Deserialize)]
                struct Payload {
                    #[serde(rename = "mediaFileId", alias = "media_file_id")]
                    media_file_id: uuid::Uuid,
                }
                let p: Payload = serde_json::from_slice(&req.payload)
                    .map_err(|e| BusError::BadRequest(format!("json decode: {e}")))?;
                let cancel = CancellationToken::new();
                crate::queue::handlers::media_file_ffprobe::run_for_file(&ctx.db, &ctx, p.media_file_id, &cancel)
                    .await
                    .map(|_| b"{}".to_vec())
                    .map_err(|e| BusError::Internal(e.to_string()))
            }
        })
        // ── image_proxy.sign_url ──────────────────────────────────────────────
        .method(decl(
            "image_proxy.sign_url",
            "Sign an external image URL for the video image proxy",
        ))
        .on_invoke("image_proxy.sign_url", move |req| {
            let ctx = ctx_proxy.clone();
            async move {
                let input: ImageProxySignInput = serde_json::from_slice(&req.payload)
                    .map_err(|e| BusError::BadRequest(format!("json decode: {e}")))?;
                let proxy_path = crate::handlers::image_proxy::to_proxy_url_force(
                    ctx.image_proxy_key(),
                    &input.url,
                );
                serde_json::to_vec(&ImageProxySignOutput { proxy_path })
                    .map_err(|e| BusError::Internal(e.to_string()))
            }
        })
        // ── capabilities ──────────────────────────────────────────────────────
        .method(decl("capabilities", "Return video bus service capabilities"))
        .on_invoke("capabilities", |_req| async move {
            serde_json::to_vec(&serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "methods": [
                    "dispatch_file_scrape",
                    "dispatch_tv_scrape",
                    "dispatch_movie_scrape",
                    "dispatch_tmdb_person_scrape",
                    "dispatch_online_video_download",
                    "dispatch_media_file_ffprobe",
                    "image_proxy.sign_url",
                    "capabilities",
                ],
            }))
            .map_err(|e| BusError::Internal(e.to_string()))
        })
}
