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
    handlers::media_stream::stream_driver_file,
    handlers::{err404, err500, err_resp, ApiResponse},
    AppState,
};

const LOCAL_MEDIA_STREAM_CHUNK_SIZE: usize = 1024 * 1024;
const REMOTE_FS_SOURCE_TYPES: [&str; 6] = ["smb", "nfs", "webdav", "ftp", "sftp", "s3"];

struct MediaFileStreamTarget {
    path: String,
    source_id: Option<String>,
    source_type: Option<String>,
    media_server_id: Option<String>,
}

pub async fn stream_media_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
    request: Request,
) -> Response {
    let session_id = match session_id_from_cookie(request.headers().get(header::COOKIE)) {
        Some(session_id) => session_id,
        None => {
            return err_resp::<()>(StatusCode::UNAUTHORIZED, "Unauthorized".into()).into_response()
        }
    };

    let db = state.sources.db_client();
    if let Err(err) = validate_session(&db, &session_id).await {
        return err_resp::<()>(StatusCode::UNAUTHORIZED, err).into_response();
    }

    let target = match load_media_file_stream_target(&db, &file_id).await {
        Ok(target) => target,
        Err(response) => return response.into_response(),
    };

    if target.media_server_id.is_some() {
        return err_resp::<()>(
            StatusCode::BAD_REQUEST,
            "Media server-backed file must be streamed via its media server".into(),
        )
        .into_response();
    }

    if target.source_type.as_deref() == Some("local") {
        let response = match ServeFile::new(&target.path)
            .with_buf_chunk_size(LOCAL_MEDIA_STREAM_CHUNK_SIZE)
            .oneshot(request)
            .await
        {
            Ok(response) => response,
            Err(never) => match never {},
        };

        return response.map(Body::new).into_response();
    }

    let Some(source_type) = target.source_type.as_deref() else {
        return err404::<()>("Filesystem-backed media file not found".into()).into_response();
    };
    if !REMOTE_FS_SOURCE_TYPES.contains(&source_type) {
        return err_resp::<()>(
            StatusCode::BAD_REQUEST,
            format!("Unsupported filesystem source type: {}", source_type),
        )
        .into_response();
    }

    let Some(source_id) = target.source_id.as_deref() else {
        return err500::<()>("Filesystem source is missing source_id".into()).into_response();
    };

    let driver = match state.sources.ensure_driver(source_id).await {
        Ok(driver) => driver,
        Err(err) => return err404::<()>(err).into_response(),
    };

    stream_driver_file(driver, target.path, request.headers().clone()).await
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

async fn load_media_file_stream_target(
    db: &Client,
    file_id: &str,
) -> Result<MediaFileStreamTarget, (StatusCode, axum::Json<ApiResponse<()>>)> {
    let row = db
        .query_opt(
            r#"
            SELECT
              mf.path,
              mf.source_id::text AS source_id,
              ms.type AS source_type,
              mf.media_server_id::text AS media_server_id
            FROM media_files mf
            LEFT JOIN media_sources ms ON ms.id = mf.source_id
            WHERE mf.id::text = $1
            "#,
            &[&file_id],
        )
        .await
        .map_err(|err| err500::<()>(format!("media file lookup failed: {}", err)))?;

    let Some(row) = row else {
        return Err(err404::<()>("Media file not found".into()));
    };

    let path: String = row
        .try_get("path")
        .map_err(|err| err500::<()>(format!("invalid media file path: {}", err)))?;
    let source_id = row
        .try_get("source_id")
        .map_err(|err| err500::<()>(format!("invalid source_id: {}", err)))?;
    let source_type = row
        .try_get("source_type")
        .map_err(|err| err500::<()>(format!("invalid source type: {}", err)))?;
    let media_server_id = row
        .try_get("media_server_id")
        .map_err(|err| err500::<()>(format!("invalid media_server_id: {}", err)))?;

    Ok(MediaFileStreamTarget {
        path,
        source_id,
        source_type,
        media_server_id,
    })
}
