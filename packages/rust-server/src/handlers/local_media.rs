use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tokio_postgres::Client;
use tower::util::ServiceExt;
use tower_http::services::ServeFile;
use tracing::error;

use crate::{
    handlers::{err404, err_resp, err500, ApiResponse},
    AppState,
};

const LOCAL_MEDIA_STREAM_CHUNK_SIZE: usize = 1024 * 1024;

pub async fn stream_local_media_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    request: Request,
) -> Response {
    let session_id = match session_id_from_cookie(request.headers().get(header::COOKIE)) {
        Some(session_id) => session_id,
        None => return err_resp::<()>(StatusCode::UNAUTHORIZED, "Unauthorized".into()).into_response(),
    };

    let db = state.sources.db_client();
    if let Err(err) = validate_session(&db, &session_id).await {
        return err_resp::<()>(StatusCode::UNAUTHORIZED, err).into_response();
    }

    let file_path = match load_local_media_path(&db, &file_id).await {
        Ok(path) => path,
        Err(response) => return response.into_response(),
    };

    let response = match ServeFile::new(&file_path)
        .with_buf_chunk_size(LOCAL_MEDIA_STREAM_CHUNK_SIZE)
        .oneshot(request)
        .await
    {
        Ok(response) => response,
        Err(never) => match never {},
    };

    response.map(Body::new).into_response()
}

fn session_id_from_cookie(cookie_header: Option<&axum::http::HeaderValue>) -> Option<String> {
    let cookie_header = cookie_header?.to_str().ok()?;
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|cookie| cookie.strip_prefix("SESSION_ID=").map(ToOwned::to_owned))
}

async fn validate_session(db: &Client, session_id: &str) -> Result<(), String> {
    let row = db
        .query_opt(
            "SELECT 1 FROM sessions WHERE id::text = $1 AND expires_at > NOW()",
            &[&session_id],
        )
        .await
        .map_err(|err| {
            error!("local media session lookup failed: {}", err);
            "Session validation failed".to_string()
        })?;

    if row.is_some() {
        Ok(())
    } else {
        Err("Unauthorized".into())
    }
}

async fn load_local_media_path(
    db: &Client,
    file_id: &str,
) -> Result<String, (StatusCode, axum::Json<ApiResponse<()>>)> {
    let row = db
        .query_opt(
            r#"
            SELECT mf.path
            FROM media_files mf
            LEFT JOIN media_sources ms ON ms.id = mf.source_id
            WHERE mf.id::text = $1
              AND ms.type = 'local'
            "#,
            &[&file_id],
        )
        .await
        .map_err(|err| err500::<()>(format!("local media lookup failed: {}", err)))?;

    let Some(row) = row else {
        return Err(err404::<()>("Local media file not found".into()));
    };

    row.try_get("path")
        .map_err(|err| err500::<()>(format!("invalid local media path: {}", err)))
}
