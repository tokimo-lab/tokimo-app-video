//! Per-record download-log broadcast bus.
//!
//! Every `append_download_log()` call publishes the new entry here after
//! writing to the JSONL file. SSE handlers for `/log-events` subscribe to
//! the corresponding record's broadcast channel and relay appends as SSE
//! frames. No DB round-trip.
//!
//! Channel entries are created on first subscribe, kept alive by an
//! Arc-refcount, and removed when the last receiver drops.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

const CHANNEL_CAPACITY: usize = 256;

/// A single appended log line, as streamed to live subscribers.
///
/// `seq` is the 1-based line number in the underlying JSONL file at the
/// time of append; combined with the backlog frame it's enough for the
/// frontend to deduplicate across the backlog→live boundary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogAppendEvent {
    pub record_id: Uuid,
    pub seq: u64,
    pub entry: serde_json::Value,
}

/// Terminal frame emitted when a record's active run ends, so live
/// subscribers can flip the UI's "running" indicator without polling
/// `/is-active`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogCompletedEvent {
    pub record_id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LogBusMessage {
    Append(LogAppendEvent),
    Completed(LogCompletedEvent),
}

struct Entry {
    refcount: AtomicUsize,
    tx: broadcast::Sender<LogBusMessage>,
    /// Monotonically increasing sequence number handed out to appends. We
    /// seed it from the existing JSONL line count on first use (done by
    /// the caller when loading the backlog).
    next_seq: AtomicUsize,
}

#[derive(Default)]
pub struct LogBus {
    entries: DashMap<Uuid, Arc<Entry>>,
}

impl LogBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to a record's log stream. Returns `(receiver, guard)`;
    /// the guard drops the refcount and removes the entry when the last
    /// subscriber disconnects.
    pub fn subscribe(self: &Arc<Self>, record_id: Uuid) -> (broadcast::Receiver<LogBusMessage>, LogGuard) {
        let entry = self
            .entries
            .entry(record_id)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
                Arc::new(Entry {
                    refcount: AtomicUsize::new(0),
                    tx,
                    next_seq: AtomicUsize::new(0),
                })
            })
            .clone();
        entry.refcount.fetch_add(1, Ordering::SeqCst);
        let rx = entry.tx.subscribe();
        let guard = LogGuard {
            bus: Arc::clone(self),
            record_id,
        };
        (rx, guard)
    }

    /// Seed the sequence counter to `n`. Called from the backlog reader
    /// so subsequent appends continue from the JSONL file's line count.
    /// The counter is only raised, never lowered — concurrent appends on
    /// a hot record still end up with monotonic seqs.
    pub fn seed_seq(&self, record_id: Uuid, n: usize) {
        if let Some(entry) = self.entries.get(&record_id) {
            let mut cur = entry.next_seq.load(Ordering::SeqCst);
            while cur < n {
                match entry
                    .next_seq
                    .compare_exchange(cur, n, Ordering::SeqCst, Ordering::SeqCst)
                {
                    Ok(_) => break,
                    Err(observed) => cur = observed,
                }
            }
        }
    }

    /// Publish an append to all live subscribers. Does nothing when no
    /// one is subscribed (no entry in the map = the SSE handler never
    /// created one for this record).
    pub fn publish_append(&self, record_id: Uuid, entry_json: serde_json::Value) {
        let Some(e) = self.entries.get(&record_id) else {
            return;
        };
        let seq = e.next_seq.fetch_add(1, Ordering::SeqCst) as u64 + 1;
        let msg = LogBusMessage::Append(LogAppendEvent {
            record_id,
            seq,
            entry: entry_json,
        });
        let _ = e.tx.send(msg);
    }

    /// Publish a completion frame. Called when the underlying run ends.
    pub fn publish_completed(&self, record_id: Uuid) {
        if let Some(e) = self.entries.get(&record_id) {
            let _ = e.tx.send(LogBusMessage::Completed(LogCompletedEvent { record_id }));
        }
    }

    fn decrement(&self, record_id: Uuid) {
        let should_remove = if let Some(e) = self.entries.get(&record_id) {
            e.refcount.fetch_sub(1, Ordering::SeqCst) == 1
        } else {
            false
        };
        if should_remove {
            self.entries
                .remove_if(&record_id, |_, e| e.refcount.load(Ordering::SeqCst) == 0);
        }
    }
}

pub struct LogGuard {
    bus: Arc<LogBus>,
    record_id: Uuid,
}

impl Drop for LogGuard {
    fn drop(&mut self) {
        self.bus.decrement(self.record_id);
    }
}
