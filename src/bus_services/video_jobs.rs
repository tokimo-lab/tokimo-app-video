//! `video_jobs` bus service — sidecar-side handler registration.
//!
//! Exposes five methods so the main server worker can hand off video jobs
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

/// Decode `{ "job": { "id": "...", "payload": {...} } }` from JSON bytes.
fn decode_request(raw: &[u8]) -> Result<(Uuid, JsonValue), BusError> {
    let v: JsonValue =
        serde_json::from_slice(raw).map_err(|e| BusError::BadRequest(format!("json decode: {e}")))?;
    let job = v
        .get("job")
        .ok_or_else(|| BusError::BadRequest("missing 'job' field".into()))?;
    let job_id = job
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BusError::BadRequest("missing 'job.id'".into()))
        .and_then(|s| Uuid::parse_str(s).map_err(|e| BusError::BadRequest(format!("job.id UUID: {e}"))))?;
    let payload = job.get("payload").cloned().unwrap_or(JsonValue::Null);
    Ok((job_id, payload))
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

    builder
        // ── dispatch_file_scrape ──────────────────────────────────────────────
        .method(decl(
            "dispatch_file_scrape",
            "Run a file_scrape job on behalf of the main worker",
        ))
        .on_invoke("dispatch_file_scrape", move |req| {
            let ctx = ctx_file.clone();
            async move {
                let (job_id, payload) = decode_request(&req.payload)?;
                let cancel = CancellationToken::new();
                crate::queue::handlers::file_scrape::handle(&ctx.db, &ctx, job_id, &payload, &cancel)
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
                let (job_id, payload) = decode_request(&req.payload)?;
                let cancel = CancellationToken::new();
                crate::queue::tv_scrape::handle(&ctx.db, &ctx, job_id, &payload, &cancel)
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
                let (job_id, payload) = decode_request(&req.payload)?;
                let cancel = CancellationToken::new();
                crate::queue::video_item_scrape::handle(&ctx.db, &ctx, job_id, &payload, &cancel)
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
                let (job_id, payload) = decode_request(&req.payload)?;
                let cancel = CancellationToken::new();
                crate::queue::tmdb_person_scrape::handle(&ctx.db, &ctx, job_id, &payload, &cancel)
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
                let (job_id, payload) = decode_request(&req.payload)?;
                let cancel = CancellationToken::new();
                crate::queue::online_media_download::handle(&ctx.db, &ctx, job_id, &payload, &cancel)
                    .await
                    .map(|_| b"{}".to_vec())
                    .map_err(|e| BusError::Internal(e.to_string()))
            }
        })
}
