use std::sync::Arc;

use sea_orm::*;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde_json::{json, Value as JsonValue};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::db::entities::download_records;
use crate::AppState;

type HandlerResult = Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>>;

const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;
const DEFAULT_POLL_RETRY_LIMIT: u32 = 3;

pub async fn handle(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    _job_id: Uuid,
    payload: &JsonValue,
) -> HandlerResult {
    let record_id = payload
        .get("recordId")
        .and_then(|v| v.as_str())
        .ok_or("Missing recordId")?;
    let url = payload
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing url")?;
    let target_library_id = payload
        .get("targetLibraryId")
        .and_then(|v| v.as_str())
        .ok_or("Missing targetLibraryId")?;

    let record_uuid = Uuid::parse_str(record_id)?;

    // Validate library exists.
    use crate::db::entities::media_libraries;
    let lib_uuid = Uuid::parse_str(target_library_id)?;
    let library = media_libraries::Entity::find_by_id(lib_uuid)
        .one(db)
        .await?;
    let Some(library) = library else {
        return Err("目标媒体库不存在".into());
    };

    // Get scraping settings for NFO generation.
    use crate::db::entities::scraping_settings;
    let scraping_row = scraping_settings::Entity::find().one(db).await?;
    let generate_nfo = scraping_row
        .as_ref()
        .map(|s| s.generate_nfo)
        .unwrap_or(false);

    // Resolve default download source root path.
    use crate::db::entities::library_file_systems;
    let lib_sources = library_file_systems::Entity::find()
        .filter(library_file_systems::Column::LibraryId.eq(lib_uuid))
        .order_by_asc(library_file_systems::Column::SortOrder)
        .all(db)
        .await?;
    let default_source = lib_sources
        .iter()
        .find(|s| s.is_default_download)
        .or(lib_sources.first());
    let organize_target_path = default_source
        .map(|s| s.root_path.clone())
        .unwrap_or_else(|| "/media".into());

    // Parse analysis from payload.
    let analysis = payload.get("analysis").cloned().unwrap_or(json!({}));
    let auth = payload.get("auth").cloned().unwrap_or(json!({}));
    let download_format = payload
        .get("downloadFormat")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");
    let media_title = payload
        .get("mediaTitle")
        .and_then(|v| v.as_str());
    let media_year = payload
        .get("mediaYear")
        .and_then(|v| v.as_str());

    let library_type = &library.r#type;
    let content_type = analysis
        .get("contentType")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let is_audio_only = download_format == "audio_only"
        || (download_format != "video"
            && (library_type == "music" || content_type == "music"));

    let settings = library
        .settings
        .as_ref()
        .cloned()
        .unwrap_or(json!({}));
    let link_mode = settings
        .get("linkMode")
        .and_then(|v| v.as_str())
        .unwrap_or("hardlink");
    let organize_lang = settings
        .get("organizeLang")
        .and_then(|v| v.as_str());

    // Build CreateTaskRequest and call TaskManager directly.
    use rust_online_media_ingest::models::CreateTaskRequest;
    use rust_online_media_ingest::runtime::spawn_task;

    let task_request = CreateTaskRequest {
        record_id: record_id.into(),
        url: url.into(),
        normalized_url: analysis
            .get("normalizedUrl")
            .and_then(|v| v.as_str())
            .map(String::from),
        provider_id: analysis
            .get("provider")
            .and_then(|p| p.get("id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        auth: Some(rust_online_media_ingest::models::TaskAuthInput {
            cookie_header: auth
                .get("cookieHeader")
                .and_then(|v| v.as_str())
                .map(String::from),
        }),
        audio_only: if is_audio_only { Some(true) } else { None },
        audio_container: None,
        target_library_id: target_library_id.into(),
        target_folder_config_snapshot:
            rust_online_media_ingest::models::TargetFolderConfigSnapshot {
                id: target_library_id.into(),
                content_type: library_type.clone(),
                download_path: organize_target_path.clone(),
                target_path: organize_target_path.clone(),
                link_mode: link_mode.into(),
                organize_lang: organize_lang.map(String::from),
            },
        metadata: rust_online_media_ingest::models::TaskMetadataInput {
            title: analysis
                .get("title")
                .and_then(|v| v.as_str())
                .map(String::from),
            media_title: media_title.map(String::from),
            media_year: media_year.map(String::from),
            thumbnail_url: analysis
                .get("thumbnailUrl")
                .and_then(|v| v.as_str())
                .map(String::from),
            duration_seconds: analysis
                .get("durationSeconds")
                .and_then(|v| v.as_u64()),
            uploader: analysis
                .get("uploader")
                .and_then(|v| v.as_str())
                .map(String::from),
            source_id: analysis
                .get("sourceId")
                .and_then(|v| v.as_str())
                .map(String::from),
            external_id: analysis
                .get("externalId")
                .and_then(|v| v.as_str())
                .map(String::from),
            source_site: analysis
                .get("sourceSite")
                .and_then(|v| v.as_str())
                .map(String::from),
            generate_nfo: Some(generate_nfo),
            raw_metadata: analysis.get("rawMetadata").cloned(),
            artist: analysis
                .get("artist")
                .and_then(|v| v.as_str())
                .map(String::from),
            album_artist: analysis
                .get("albumArtist")
                .and_then(|v| v.as_str())
                .map(String::from),
            album: analysis
                .get("album")
                .and_then(|v| v.as_str())
                .map(String::from),
            track_title: analysis
                .get("trackTitle")
                .and_then(|v| v.as_str())
                .map(String::from),
            track_number: analysis
                .get("trackNumber")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            disc_number: analysis
                .get("discNumber")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            genre: analysis
                .get("genre")
                .and_then(|v| v.as_str())
                .map(String::from),
            release_date: analysis
                .get("releaseDate")
                .and_then(|v| v.as_str())
                .map(String::from),
        },
    };

    let task_id = state
        .online_media
        .tasks
        .create_task(task_request.clone())
        .await;
    spawn_task((*state.online_media).clone(), task_id.clone(), task_request);

    info!(record_id, task_id, "Online media task created");

    // Update download record with task ID.
    let now: DateTimeWithTimeZone = chrono::Utc::now().into();
    download_records::Entity::update_many()
        .col_expr(
            download_records::Column::RustTaskId,
            sea_orm::sea_query::Expr::value(&task_id),
        )
        .col_expr(
            download_records::Column::Status,
            sea_orm::sea_query::Expr::value("downloading"),
        )
        .col_expr(
            download_records::Column::ImportStatus,
            sea_orm::sea_query::Expr::value("importing"),
        )
        .col_expr(
            download_records::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .filter(download_records::Column::Id.eq(record_uuid))
        .exec(db)
        .await?;

    // Poll task status until completion or failure.
    let poll_interval = std::env::var("ONLINE_MEDIA_POLL_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_POLL_INTERVAL_MS);
    let poll_retry_limit = std::env::var("ONLINE_MEDIA_POLL_RETRY_LIMIT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(DEFAULT_POLL_RETRY_LIMIT);

    let mut poll_failure_count: u32 = 0;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(poll_interval)).await;

        let task = state.online_media.tasks.get_task(&task_id).await;
        let Some(task) = task else {
            poll_failure_count += 1;
            warn!(
                task_id,
                attempt = poll_failure_count,
                "Task not found during polling"
            );
            if poll_failure_count > poll_retry_limit {
                let err_msg = format!(
                    "在线媒体任务状态已丢失，可能是服务重启导致任务中断 ({task_id})"
                );
                update_record_failed(db, record_uuid, &err_msg).await;
                return Err(err_msg.into());
            }
            continue;
        };

        poll_failure_count = 0;
        let resp = task.to_response();

        // Update download record with progress.
        let progress_str = to_record_progress(resp.progress);
        let update_now: DateTimeWithTimeZone = chrono::Utc::now().into();
        let import_status = match resp.status {
            rust_online_media_ingest::models::TaskState::Completed => "completed",
            rust_online_media_ingest::models::TaskState::Failed => "failed",
            _ => "importing",
        };

        let mut update = download_records::ActiveModel {
            id: Set(record_uuid),
            ..Default::default()
        };
        update.rust_task_id = Set(Some(task_id.clone()));
        update.progress = Set(Some(progress_str));
        if let Some(bytes) = resp.downloaded_bytes {
            update.downloaded_size = Set(Some(bytes.to_string()));
        }
        if let Some(bytes) = resp.total_bytes {
            update.file_size = Set(Some(bytes.to_string()));
        }
        if let Some(ref mp) = resp.manifest_path {
            update.manifest_path = Set(Some(mp.clone()));
        }
        if let Some(ref tp) = resp.target_path {
            update.target_path = Set(Some(tp.clone()));
        }
        update.import_status = Set(Some(import_status.into()));
        if let Some(ref e) = resp.error {
            update.import_error = Set(Some(e.clone()));
        }
        update.updated_at = Set(Some(update_now));
        update.update(db).await?;

        // Progress updates are automatically broadcast via the job worker's
        // mark_completed / mark_failed lifecycle. For intermediate progress,
        // we update the job row directly so the SSE stream picks it up.
        if let Ok(Some(model)) =
            crate::db::repos::job_repo::JobRepo::update_progress(
                db,
                _job_id,
                resp.progress.map(|p| p.round() as i32).unwrap_or(0),
                Some(json!({
                    "recordId": record_id,
                    "taskId": task_id,
                    "stage": resp.stage,
                })),
            )
            .await
        {
            let _ = state.event_tx.send(crate::queue::AppEvent::JobUpdate {
                job: crate::db::models::job::JobOutput::from(model),
            });
        }

        match resp.status {
            rust_online_media_ingest::models::TaskState::Completed => {
                info!(record_id, task_id, "Online media task completed");

                let final_status = if resp.target_path.is_some() {
                    "organized"
                } else {
                    "completed"
                };

                let done_now: DateTimeWithTimeZone = chrono::Utc::now().into();
                let mut done_update = download_records::ActiveModel {
                    id: Set(record_uuid),
                    ..Default::default()
                };
                done_update.status = Set(final_status.into());
                done_update.progress = Set(Some("1".into()));
                done_update.import_status = Set(Some("completed".into()));
                done_update.manifest_path = Set(None);
                if let Some(ref tp) = resp.target_path {
                    done_update.target_path = Set(Some(tp.clone()));
                }
                done_update.updated_at = Set(Some(done_now));
                done_update.update(db).await?;

                return Ok(Some(json!({
                    "taskId": task_id,
                    "targetPath": resp.target_path,
                    "manifestPath": null,
                })));
            }
            rust_online_media_ingest::models::TaskState::Failed
            | rust_online_media_ingest::models::TaskState::Cancelled => {
                let message = resp.error.unwrap_or_else(|| "在线媒体下载失败".into());
                error!(record_id, task_id, %message, "Online media task failed");
                update_record_failed(db, record_uuid, &message).await;
                return Err(message.into());
            }
            _ => {
                // Still running — continue polling.
            }
        }
    }
}

fn to_record_progress(progress: Option<f64>) -> String {
    match progress {
        Some(p) if !p.is_nan() => {
            let ratio = if p > 1.0 { p / 100.0 } else { p };
            format!("{}", ratio.clamp(0.0, 1.0))
        }
        _ => "0".into(),
    }
}

async fn update_record_failed(db: &DatabaseConnection, record_id: Uuid, message: &str) {
    let now: DateTimeWithTimeZone = chrono::Utc::now().into();
    let result = download_records::Entity::update_many()
        .col_expr(
            download_records::Column::Status,
            sea_orm::sea_query::Expr::value("failed"),
        )
        .col_expr(
            download_records::Column::ImportStatus,
            sea_orm::sea_query::Expr::value("failed"),
        )
        .col_expr(
            download_records::Column::ImportError,
            sea_orm::sea_query::Expr::value(message),
        )
        .col_expr(
            download_records::Column::ManifestPath,
            sea_orm::sea_query::Expr::value(Option::<String>::None),
        )
        .col_expr(
            download_records::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .filter(download_records::Column::Id.eq(record_id))
        .exec(db)
        .await;

    if let Err(e) = result {
        error!(
            %record_id,
            "Failed to update download record to failed state: {e}"
        );
    }
}
