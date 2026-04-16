use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;
use crate::db::repos::media_playback_state_repo::MediaPlaybackStateRepo;
use crate::handlers::user::AuthUser;
use crate::handlers::{ok, ok_empty};

/// GET /api/playback/state — load persisted playback state
pub async fn get_playback_state(State(state): State<Arc<AppState>>, AuthUser(auth): AuthUser) -> Response {
    let user_id: Uuid = match auth.user_id.parse() {
        Ok(u) => u,
        Err(_) => return axum::http::StatusCode::BAD_REQUEST.into_response(),
    };

    match MediaPlaybackStateRepo::get(&state.db, user_id).await {
        Ok(Some(data)) => ok(data).into_response(),
        Ok(None) => ok(serde_json::json!({})).into_response(),
        Err(e) => e.into_response(),
    }
}

#[derive(Deserialize)]
pub struct SavePlaybackStateInput {
    #[serde(rename = "stateData")]
    pub state_data: serde_json::Value,
}

/// POST /api/playback/state — save playback state (also used by sendBeacon)
pub async fn save_playback_state(
    State(state): State<Arc<AppState>>,
    AuthUser(auth): AuthUser,
    axum::Json(body): axum::Json<SavePlaybackStateInput>,
) -> Response {
    let user_id: Uuid = match auth.user_id.parse() {
        Ok(u) => u,
        Err(_) => return axum::http::StatusCode::BAD_REQUEST.into_response(),
    };

    match MediaPlaybackStateRepo::upsert(&state.db, user_id, body.state_data).await {
        Ok(()) => ok_empty().into_response(),
        Err(e) => e.into_response(),
    }
}
