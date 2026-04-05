use crate::db::ApiDateTimeExt;
use std::{convert::Infallible, env, sync::Arc};

use async_stream::stream;
use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use bytes::Bytes;
use futures_util::stream::Stream;
use serde::Deserialize;
use subtitle_aggregator::models::{
    SubtitleDownloadRequest as AggDownloadRequest, SubtitleSearchRequest,
};
use tokio::sync::mpsc;

use crate::{
    db::models::subtitle::SubtitleRecord,
    db::repos::subtitle_repo::{CreateSubtitleInput, SubtitleRepo},
    handlers::{err500, ok, ApiResponse},
    services::storage::UploadOptions,
    AppState,
};

// ── Download request wrapper (adds file_id + aggregator routing fields) ───────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleDownloadHandlerRequest {
    pub file_id: String,
    pub subtitle_id: String,
    pub detail_path: Option<String>,
    pub download_path: Option<String>,
    pub language: String,
    pub format: String,
    pub name: Option<String>,
    #[serde(default = "default_provider")]
    pub provider: String,
}

fn default_provider() -> String {
    "assrt".into()
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn storage_base_url() -> String {
    env::var("VITE_STORAGE_URL")
        .or_else(|_| env::var("STORAGE_BASE_URL"))
        .unwrap_or_else(|_| "http://localhost:5678/storage".to_string())
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/subtitles/file/:file_id — list all subtitles attached to a file.
pub async fn get_file_subtitles(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
) -> Result<
    Json<ApiResponse<Vec<SubtitleRecord>>>,
    (
        axum::http::StatusCode,
        Json<ApiResponse<Vec<SubtitleRecord>>>,
    ),
> {
    let base = storage_base_url();
    match SubtitleRepo::get_all_file_subtitles(&state.db, &file_id, &base).await {
        Ok(records) => Ok(ok(records)),
        Err(e) => Err(err500(e.to_string())),
    }
}

/// POST /api/subtitles/search — SSE streaming multi-provider subtitle search.
/// Each SSE event is a JSON array of SubtitleSearchResult from one provider.
pub async fn search(
    State(state): State<Arc<AppState>>,
    Json(input): Json<SubtitleSearchRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, mut rx) = mpsc::channel::<Vec<subtitle_aggregator::models::SubtitleSearchResult>>(50);
    let aggregator = Arc::clone(&state.subtitle_aggregator);

    tokio::spawn(async move {
        aggregator.search_streaming(input, tx).await;
    });

    let s = stream! {
        while let Some(results) = rx.recv().await {
            if let Ok(data) = serde_json::to_string(&results) {
                yield Ok::<Event, Infallible>(Event::default().data(data));
            }
        }
    };

    Sse::new(s).keep_alive(KeepAlive::default())
}

/// POST /api/subtitles/download — download via aggregator, save to storage + DB.
pub async fn download(
    State(state): State<Arc<AppState>>,
    Json(input): Json<SubtitleDownloadHandlerRequest>,
) -> Result<
    Json<ApiResponse<SubtitleRecord>>,
    (axum::http::StatusCode, Json<ApiResponse<SubtitleRecord>>),
> {
    let agg_request = AggDownloadRequest {
        subtitle_id: input.subtitle_id.clone(),
        detail_path: input.detail_path.clone(),
        download_path: input.download_path.clone(),
        language: input.language.clone(),
        format: input.format.clone(),
        name: input.name.clone(),
        provider: input.provider.clone(),
    };

    // 1. Download subtitle content via aggregator (returns base64-encoded bytes)
    let downloaded = match state.subtitle_aggregator.download(&agg_request).await {
        Ok(d) => d,
        Err(e) => return Err(err500(format!("字幕下载失败: {e}"))),
    };

    // 2. Decode base64 content
    let content_bytes = match BASE64.decode(&downloaded.content_base64) {
        Ok(b) => b,
        Err(e) => return Err(err500(format!("字幕内容解码失败: {e}"))),
    };

    let format = downloaded.format.clone();
    let s3_key = format!(
        "subtitles/{}/{}.{}",
        input.file_id,
        uuid::Uuid::new_v4(),
        format
    );

    let content_type = if format == "vtt" {
        "text/vtt; charset=utf-8".to_string()
    } else {
        "text/plain; charset=utf-8".to_string()
    };

    // 3. Upload to storage
    if let Err(e) = state
        .storage
        .upload(
            &s3_key,
            Bytes::from(content_bytes),
            Some(UploadOptions {
                content_type: Some(content_type),
            }),
        )
        .await
    {
        return Err(err500(format!("字幕上传存储失败: {e}")));
    }

    let title = input.name.or(Some(downloaded.name));

    // 4. Save to DB
    let row = match SubtitleRepo::create_subtitle(
        &state.db,
        CreateSubtitleInput {
            file_id: input.file_id.clone(),
            language: input.language.clone(),
            title,
            format: format.clone(),
            source: input.provider.clone(),
            source_id: Some(input.subtitle_id.clone()),
            s3_key: s3_key.clone(),
        },
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return Err(err500(format!("字幕记录保存失败: {e}"))),
    };

    let base = storage_base_url();
    let storage_url = Some(format!("{}/{s3_key}", base.trim_end_matches('/')));

    let record = SubtitleRecord {
        id: row.id.to_string(),
        language: row.language,
        title: row.title,
        source_type: row.source_type,
        format: row.format,
        is_default: row.is_default,
        is_forced: row.is_forced,
        is_hearing_impaired: row.is_hearing_impaired,
        stream_index: None,
        storage_url,
        source: row.source,
        created_at: row.created_at.to_api_datetime(),
    };

    Ok(ok(record))
}

/// DELETE /api/subtitles/:subtitle_id — delete a subtitle record and its stored file.
pub async fn delete_subtitle(
    State(state): State<Arc<AppState>>,
    Path(subtitle_id): Path<String>,
) -> Result<
    Json<ApiResponse<serde_json::Value>>,
    (axum::http::StatusCode, Json<ApiResponse<serde_json::Value>>),
> {
    match SubtitleRepo::delete_subtitle(&state.db, &subtitle_id).await {
        Ok(Some(s3_key)) => {
            let _ = state.storage.delete(&s3_key).await;
            Ok(ok(serde_json::json!({ "ok": true })))
        }
        Ok(None) => Ok(ok(serde_json::json!({ "ok": true }))),
        Err(e) => Err(err500(e.to_string())),
    }
}

// ── Download request wrapper (adds file_id + aggregator routing fields) ───────
