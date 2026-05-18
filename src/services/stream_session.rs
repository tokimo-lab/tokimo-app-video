//! Playback stream session manager.
//!
//! Every streaming request (direct play or HLS) registers a `CancellationToken`
//! keyed by `file_id`. When a session ends — either because the user explicitly
//! calls `stop-session`, or because all in-flight HTTP stream connections close —
//! the token is cancelled. All spawned stream tasks (`vfs-task`, `tee-task`) select
//! on this token and exit immediately, releasing channel buffers, SMB connections,
//! and subtitle tap data.
//!
//! A background cleanup pass (`cleanup_stale`) is run by the scheduler to
//! reap sessions whose last activity was more than `STREAM_SESSION_TTL` ago,
//! providing a safety net for browsers that disconnect without calling stop-session
//! (e.g., hard crash, network loss).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

const STREAM_SESSION_TTL: Duration = Duration::from_mins(30); // safety-net only; normal cleanup via drop_guard
const CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

struct Entry {
    token: CancellationToken,
    last_seen: Instant,
}

#[derive(Clone)]
pub struct StreamSessionManager {
    inner: Arc<Mutex<HashMap<String, Entry>>>,
}

impl Default for StreamSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamSessionManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Return the active `CancellationToken` for `file_id`, creating one if
    /// none exists or the previous one was already cancelled.
    pub fn create_or_get(&self, file_id: &str) -> CancellationToken {
        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(file_id.to_string()).or_insert_with(|| {
            info!("[StreamSession] created session file_id={}", file_id);
            Entry {
                token: CancellationToken::new(),
                last_seen: Instant::now(),
            }
        });
        // If a previous token was cancelled (session was stopped) create a fresh one.
        if entry.token.is_cancelled() {
            info!(
                "[StreamSession] re-created session (previous cancelled) file_id={}",
                file_id
            );
            entry.token = CancellationToken::new();
        }
        entry.last_seen = Instant::now();
        entry.token.clone()
    }

    /// Record activity for `file_id` (called on each streaming chunk / heartbeat).
    /// Prevents stale cleanup from reaping an active stream.
    pub fn touch(&self, file_id: &str) {
        let mut map = self.inner.lock().unwrap();
        if let Some(e) = map.get_mut(file_id) {
            e.last_seen = Instant::now();
        }
    }

    /// Cancel all stream tasks for `file_id` and remove the entry.
    pub fn cancel(&self, file_id: &str) {
        let entry = self.inner.lock().unwrap().remove(file_id);
        if let Some(e) = entry {
            if e.token.is_cancelled() {
                debug!(
                    "[StreamSession] cancel called on already-cancelled session file_id={}",
                    file_id
                );
            } else {
                info!("[StreamSession] cancelling stream session file_id={}", file_id);
                e.token.cancel();
            }
        } else {
            debug!("[StreamSession] cancel called but no session found file_id={}", file_id);
        }
    }

    /// Cancel and remove sessions that have been idle for longer than
    /// `STREAM_SESSION_TTL`. Called periodically by the cleanup task.
    pub fn cleanup_stale(&self) {
        let now = Instant::now();
        let stale: Vec<_> = {
            let map = self.inner.lock().unwrap();
            map.iter()
                .filter(|(_, e)| {
                    // Primary: token already cancelled (stream ended naturally) — remove the entry.
                    // Safety net: token still alive but session is too old (shouldn't normally happen).
                    e.token.is_cancelled() || now.duration_since(e.last_seen) > STREAM_SESSION_TTL
                })
                .map(|(k, _)| k.clone())
                .collect()
        };
        if stale.is_empty() {
            return;
        }
        warn!(
            "[StreamSession] cleanup: {} stale session(s) idle >{:?}: {:?}",
            stale.len(),
            STREAM_SESSION_TTL,
            stale
        );
        for file_id in stale {
            self.cancel(&file_id);
        }
    }

    /// Spawn a background task that runs `cleanup_stale` on a fixed interval.
    /// Call this once at server startup (mirrors `HlsSessionManager::start_cleanup_task`).
    pub fn start_cleanup_task(&self) {
        let mgr = self.clone(); // cheap: only clones the inner Arc
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
            interval.tick().await; // skip first immediate tick
            loop {
                interval.tick().await;
                mgr.cleanup_stale();
            }
        });
    }
}
