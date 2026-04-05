use axum::{
    extract::{Path, Query, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
};
use std::sync::Arc;
use tracing::{info, trace};

use crate::AppState;

pub async fn get_subtitle_events(
    State(state): State<Arc<AppState>>,
    Path(subtitle_id): Path<String>,
    Query(query): Query<rust_subtitle::types::SubtitleEventsQuery>,
) -> Response {
    let start_ms = query.start_ms.unwrap_or(0.0) as i64;
    let end_ms = query.end_ms.unwrap_or(i64::MAX as f64) as i64;

    if let Some((events, complete)) = state.subtitle_cache.query(&subtitle_id, start_ms, end_ms) {
        let body = serde_json::json!({ "events": events, "complete": complete });
        axum::Json(body).into_response()
    } else {
        let body = serde_json::json!({ "events": [], "complete": false });
        axum::Json(body).into_response()
    }
}

pub async fn subtitle_events_sse(
    State(state): State<Arc<AppState>>,
    Path(subtitle_id): Path<String>,
    Query(_query): Query<rust_subtitle::types::SubtitleEventsQuery>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let (snapshot, mut rx) = state.subtitle_cache.subscribe(&subtitle_id);

    info!(
        "[SSE] subscriber connected for sub={}, snapshot={} events",
        subtitle_id,
        snapshot.len()
    );

    let sub_id = subtitle_id.clone();
    let stream = async_stream::stream! {
        for ev in &snapshot {
            let json = serde_json::to_string(ev).unwrap_or_default();
            if ev.data.is_some() {
                yield Ok::<_, std::convert::Infallible>(Event::default().event("pgs").data(json));
            } else {
                yield Ok::<_, std::convert::Infallible>(Event::default().data(json));
            }
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    trace!("[SSE] pushing event to sub={}: timeMs={}", sub_id, ev.time_ms);
                    let json = serde_json::to_string(&ev).unwrap_or_default();
                    if ev.data.is_some() {
                        yield Ok(Event::default().event("pgs").data(json));
                    } else {
                        yield Ok(Event::default().data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    info!("[SSE] subtitle {} lagged {} events", sub_id, n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("[SSE] broadcast closed for sub={}", sub_id);
                    break;
                }
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}