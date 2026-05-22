use std::sync::Arc;

use bytes::Bytes;
use sea_orm::*;
use serde_json::{Value as JsonValue, json};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::bus_clients::downloader::{CompleteDownloaderRequest, UpdateDownloaderStatusRequest};
use crate::db::entities::{vfs, video_files};
use crate::db::repos::download_record_repo::DownloadRecordRepo;
use crate::db::repos::job_repo::JobRepo;
use crate::queue::cancellation::{JobCancel, check_cancel};
use crate::services::storage::UploadOptions;

// ── Download log helpers ──────────────────────────────────────────────────────

fn download_log_key(record_id: &Uuid) -> String {
    format!("logs/download/{record_id}.jsonl")
}

async fn append_download_log(
    state: &Arc<AppState>,
    record_id: &Uuid,
    run_id: &str,
    phase: &str,
    message: &str,
    details: Option<JsonValue>,
) {
    let key = download_log_key(record_id);
    let entry = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "recordId": record_id.to_string(),
        "runId": run_id,
        "phase": phase,
        "message": message,
        "details": details,
    });
    let new_line = format!("{}\n", serde_json::to_string(&entry).unwrap_or_default());

    // Download existing content, append new line, re-upload.
    let existing = state.storage.download(&key).await.unwrap_or_default();
    let mut content = existing.to_vec();
    content.extend_from_slice(new_line.as_bytes());

    if let Err(e) = state
        .storage
        .upload(
            &key,
            Bytes::from(content),
            Some(UploadOptions {
                content_type: Some("application/x-ndjson".into()),
            }),
        )
        .await
    {
        warn!(%record_id, "Failed to write download log: {e}");
    }

    // Fan out to any live SSE subscribers. Cheap no-op when no one is watching.
    state.download_log_bus.publish_append(*record_id, entry);

    // "completed"/"error" are terminal phases — notify subscribers so the UI
    // can flip the "running" indicator without polling `/is-active`.
    if phase == "completed" || phase == "error" {
        state.download_log_bus.publish_completed(*record_id);
    }
}

type HandlerResult = Result<Option<JsonValue>, Box<dyn std::error::Error + Send + Sync>>;

const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;
const DEFAULT_POLL_RETRY_LIMIT: u32 = 3;

async fn push_downloader_status(state: &Arc<AppState>, request: UpdateDownloaderStatusRequest) {
    let Some(client) = state.bus_client.get() else { return };
    if let Err(error) = crate::bus_clients::downloader::update_status(client, &request).await {
        error!(%error, record_id = %request.record_id, "failed to push downloader status to host");
    }
}

async fn push_downloader_complete(state: &Arc<AppState>, request: CompleteDownloaderRequest) {
    let Some(client) = state.bus_client.get() else { return };
    if let Err(error) = crate::bus_clients::downloader::complete(client, &request).await {
        warn!(%error, record_id = %request.record_id, "failed to push downloader completion to host");
    }
}

#[allow(clippy::too_many_lines)]
pub async fn handle(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    job_id: Uuid,
    payload: &JsonValue,
    cancel: &JobCancel,
    user_id: Option<Uuid>,
) -> HandlerResult {
    check_cancel(cancel)?;
    let record_id = payload
        .get("recordId")
        .and_then(|v| v.as_str())
        .ok_or("Missing recordId")?;
    let url = payload.get("url").and_then(|v| v.as_str()).ok_or("Missing url")?;
    let target_app_id = payload
        .get("targetAppId")
        .and_then(|v| v.as_str())
        .ok_or("Missing targetAppId")?;

    let record_uuid = Uuid::parse_str(record_id)?;
    let job_owner_user_id = user_id;

    // Resolve target library from videos table only (music/book entities removed in B4)
    use crate::db::entities::videos;
    use crate::db::repos::media::VideoRepo;
    let lib_uuid = Uuid::parse_str(target_app_id)?;

    struct LibInfo {
        r#type: String,
        settings: Option<serde_json::Value>,
        sources: serde_json::Value,
    }

    let lib_info = if let Some(v) = videos::Entity::find_by_id(lib_uuid).one(db).await? {
        LibInfo {
            r#type: v.r#type,
            settings: v.settings,
            sources: v.sources,
        }
    } else {
        // Music/book support removed - entities no longer available in video app
        return Err("目标应用不存在或不支持（video app 仅支持 video 类型）".into());
    };

    // Get scraping settings for NFO generation.
    use crate::config::ScrapingSettings;
    use crate::db::repos::scrape_settings_repo::ScrapeSettingsRepo;
    let scraping_row = ScrapeSettingsRepo::get::<ScrapingSettings>(db).await.ok();
    let generate_nfo = scraping_row.as_ref().is_some_and(|s| s.generate_nfo);

    // Resolve default download source root path from sources JSON.
    let parsed_sources = VideoRepo::parse_sources(&lib_info.sources);
    if parsed_sources.is_empty() {
        let err_msg = "该应用未配置文件系统源，请先在应用设置中添加至少一个文件系统路径";
        update_record_failed(state, record_uuid, err_msg).await;
        let run_id = record_uuid.to_string();
        append_download_log(state, &record_uuid, &run_id, "error", err_msg, None).await;
        return Err("该应用未配置文件系统源".into());
    }
    let default_source = parsed_sources.iter().find(|s| s.2).or(parsed_sources.first());
    let download_source_id = default_source.map(|s| s.0);
    let organize_target_path = default_source.map(|s| s.1.clone()).unwrap_or_default();

    // Fetch file system config (root_folder_path) for computing relative paths.
    // Also detect whether the source is local (can be accessed via tokio::fs) or
    // requires VFS (SMB, SFTP, S3, etc.).
    let (fs_driver_root, fs_source_type) = if let Some(sid) = download_source_id {
        let fs = vfs::Entity::find_by_id(sid).one(db).await?;
        let root = fs.as_ref().and_then(crate::handlers::media::utils::local_driver_root);
        let fs_type = fs.map_or_else(|| "local".into(), |f| f.r#type);
        (root, fs_type)
    } else {
        (None, "local".into())
    };

    // For non-local sources (SMB, SFTP, S3…) yt-dlp cannot write to the VFS path
    // directly. We use a temporary local staging directory as the download target,
    // then push the organised files to the VFS after the task completes.
    let is_local_source_fs = fs_source_type == "local";
    let vfs_stage_dir: Option<std::path::PathBuf> = if is_local_source_fs {
        None
    } else {
        Some(
            state
                .online_media
                .staging_root
                .join(format!("{record_uuid}-vfs-target")),
        )
    };
    let effective_target_path = vfs_stage_dir
        .as_ref()
        .map_or_else(|| organize_target_path.clone(), |p| p.to_string_lossy().into_owned());

    // Parse analysis from payload.
    let analysis = payload.get("analysis").cloned().unwrap_or(json!({}));
    let auth = payload.get("auth").cloned().unwrap_or(json!({}));
    let download_format = payload.get("downloadFormat").and_then(|v| v.as_str()).unwrap_or("auto");
    let media_title = payload.get("mediaTitle").and_then(|v| v.as_str());
    let media_year = payload.get("mediaYear").and_then(|v| v.as_str());

    let app_type = &lib_info.r#type;
    let is_audio_only = download_format == "audio_only" || (download_format != "video" && app_type == "music");

    let settings = lib_info.settings.clone().unwrap_or(json!({}));
    let link_mode = settings.get("linkMode").and_then(|v| v.as_str()).unwrap_or("hardlink");
    let organize_lang = settings.get("organizeLang").and_then(|v| v.as_str());

    // Build CreateTaskRequest and call TaskManager directly.
    use rust_online_media_ingest::models::CreateTaskRequest;
    use rust_online_media_ingest::runtime::spawn_task;

    let task_request = CreateTaskRequest {
        record_id: record_id.into(),
        url: url.into(),
        normalized_url: analysis.get("normalizedUrl").and_then(|v| v.as_str()).map(String::from),
        provider_id: analysis
            .get("provider")
            .and_then(|p| p.get("id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        auth: Some(rust_online_media_ingest::models::TaskAuthInput {
            cookie_header: auth.get("cookieHeader").and_then(|v| v.as_str()).map(String::from),
        }),
        audio_only: if is_audio_only { Some(true) } else { None },
        audio_container: None,
        target_library_id: target_app_id.into(),
        target_folder_config_snapshot: rust_online_media_ingest::models::TargetFolderConfigSnapshot {
            id: target_app_id.into(),
            content_type: app_type.clone(),
            download_path: effective_target_path.clone(),
            target_path: effective_target_path.clone(),
            link_mode: link_mode.into(),
            organize_lang: organize_lang.map(String::from),
        },
        metadata: rust_online_media_ingest::models::TaskMetadataInput {
            title: analysis.get("title").and_then(|v| v.as_str()).map(String::from),
            media_title: media_title.map(String::from),
            media_year: media_year.map(String::from),
            thumbnail_url: analysis
                .get("thumbnailUrl")
                .and_then(|v| v.as_str())
                .map(|url| crate::handlers::image_proxy::unwrap_proxy_url(url).unwrap_or_else(|| url.to_string())),
            duration_seconds: analysis.get("durationSeconds").and_then(sea_orm::JsonValue::as_u64),
            uploader: analysis.get("uploader").and_then(|v| v.as_str()).map(String::from),
            source_id: analysis.get("sourceId").and_then(|v| v.as_str()).map(String::from),
            external_id: analysis.get("externalId").and_then(|v| v.as_str()).map(String::from),
            source_site: analysis.get("sourceSite").and_then(|v| v.as_str()).map(String::from),
            generate_nfo: Some(generate_nfo),
            raw_metadata: analysis.get("rawMetadata").cloned(),
            artist: analysis.get("artist").and_then(|v| v.as_str()).map(String::from),
            album_artist: analysis.get("albumArtist").and_then(|v| v.as_str()).map(String::from),
            album: analysis.get("album").and_then(|v| v.as_str()).map(String::from),
            track_title: analysis.get("trackTitle").and_then(|v| v.as_str()).map(String::from),
            track_number: analysis
                .get("trackNumber")
                .and_then(sea_orm::JsonValue::as_u64)
                .map(|v| v as u32),
            disc_number: analysis
                .get("discNumber")
                .and_then(sea_orm::JsonValue::as_u64)
                .map(|v| v as u32),
            genre: analysis.get("genre").and_then(|v| v.as_str()).map(String::from),
            release_date: analysis.get("releaseDate").and_then(|v| v.as_str()).map(String::from),
        },
    };

    let task_id = state.online_media.tasks.create_task(task_request.clone()).await;
    spawn_task((*state.online_media).clone(), task_id.clone(), task_request);

    info!(record_id, task_id, "Online media task created");
    append_download_log(
        state,
        &record_uuid,
        &task_id,
        "download-started",
        &format!("开始下载: {url}"),
        Some(json!({ "taskId": task_id })),
    )
    .await;

    state.download_tasks.lock().await.insert(record_uuid, task_id.clone());
    push_downloader_status(
        state,
        UpdateDownloaderStatusRequest {
            record_id: record_uuid,
            status: Some("downloading".into()),
            progress: Some(0.0),
            downloaded_bytes: Some(0),
            download_speed: None,
            eta_seconds: None,
            thumbnail_url: analysis.get("thumbnailUrl").and_then(|v| v.as_str()).map(String::from),
            error_message: None,
        },
    )
    .await;

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
    let mut last_logged_stage: Option<String> = None;

    loop {
        check_cancel(cancel)?;
        tokio::time::sleep(tokio::time::Duration::from_millis(poll_interval)).await;

        let task = state.online_media.tasks.get_task(&task_id).await;
        let Some(task) = task else {
            poll_failure_count += 1;
            warn!(task_id, attempt = poll_failure_count, "Task not found during polling");
            if poll_failure_count > poll_retry_limit {
                let err_msg = format!("在线媒体任务状态已丢失，可能是服务重启导致任务中断 ({task_id})");
                append_download_log(state, &record_uuid, &task_id, "error", &err_msg, None).await;
                update_record_failed(state, record_uuid, &err_msg).await;
                return Err(err_msg.into());
            }
            continue;
        };

        poll_failure_count = 0;
        let resp = task.to_response();

        // Write a log entry whenever the stage changes.
        if resp.stage != last_logged_stage
            && let Some(ref stage) = resp.stage
        {
            let phase = stage_to_log_phase(stage);
            let msg = stage_to_log_message(stage, resp.progress);
            append_download_log(state, &record_uuid, &task_id, phase, &msg, None).await;
            last_logged_stage = resp.stage.clone();
        }

        push_downloader_status(
            state,
            UpdateDownloaderStatusRequest {
                record_id: record_uuid,
                status: Some(match resp.status {
                    rust_online_media_ingest::models::TaskState::Completed => "completed".into(),
                    rust_online_media_ingest::models::TaskState::Failed => "failed".into(),
                    rust_online_media_ingest::models::TaskState::Cancelled => "cancelled".into(),
                    _ => "downloading".into(),
                }),
                progress: Some(to_record_progress(resp.progress)),
                downloaded_bytes: resp.downloaded_bytes.and_then(|bytes| i64::try_from(bytes).ok()),
                download_speed: resp.speed_bytes.and_then(|bytes| i64::try_from(bytes).ok()),
                eta_seconds: resp.eta_seconds.and_then(|eta| i32::try_from(eta).ok()),
                thumbnail_url: None,
                error_message: resp.error.clone(),
            },
        )
        .await;

        // Progress updates are automatically broadcast via the job worker's
        // mark_completed / mark_failed lifecycle. For intermediate progress,
        // we update the job row directly so the SSE stream picks it up.
        if let Ok(Some(model)) = crate::db::repos::job_repo::JobRepo::update_progress_via_bus(
            state,
            job_id,
            resp.progress.map_or(0, |p| p.round() as i32),
            Some(json!({
                "recordId": record_id,
                "taskId": task_id,
                "stage": resp.stage,
            })),
            user_id,
        )
        .await
        {
            let job_out = crate::db::models::job::JobOutput::from(model);
            state.bus_notify_job(&job_out);
            let _ = state
                .event_tx
                .send(crate::queue::AppEvent::JobUpdate { job: Box::new(job_out) });
        }

        match resp.status {
            rust_online_media_ingest::models::TaskState::Completed => {
                info!(record_id, task_id, "Online media task completed");

                // For non-local sources, copy organised files from local staging to VFS.
                let vfs_copy_result: Option<VfsCopyResult> =
                    if let (Some(stage_dir), Some(sid)) = (&vfs_stage_dir, download_source_id) {
                        match copy_staged_to_vfs(
                            &state.sources,
                            state,
                            &sid.to_string(),
                            stage_dir,
                            &organize_target_path,
                            &record_uuid,
                            &task_id,
                        )
                        .await
                        {
                            Ok(result) => {
                                // Clean up local staging dir after successful VFS copy.
                                let _ = tokio::fs::remove_dir_all(stage_dir).await;
                                Some(result)
                            }
                            Err(e) => {
                                let msg = format!("上传到 VFS 失败: {e}");
                                error!(record_id, task_id, %e, "Failed to copy to VFS");
                                append_download_log(state, &record_uuid, &task_id, "error", &msg, None).await;
                                update_record_failed(state, record_uuid, &msg).await;
                                return Err(msg.into());
                            }
                        }
                    } else {
                        None
                    };

                let vfs_target_path = vfs_copy_result
                    .as_ref()
                    .map(|r| r.target_path.clone())
                    .or_else(|| resp.target_path.clone());

                append_download_log(
                    state,
                    &record_uuid,
                    &task_id,
                    "completed",
                    "下载完成",
                    vfs_target_path.as_ref().map(|tp| json!({ "targetPath": tp })),
                )
                .await;

                let final_status = if vfs_copy_result.is_some() {
                    "organized"
                } else {
                    "completed"
                };

                push_downloader_status(
                    state,
                    UpdateDownloaderStatusRequest {
                        record_id: record_uuid,
                        status: Some(final_status.into()),
                        progress: Some(1.0),
                        downloaded_bytes: resp.downloaded_bytes.and_then(|bytes| i64::try_from(bytes).ok()),
                        download_speed: Some(0),
                        eta_seconds: Some(0),
                        thumbnail_url: None,
                        error_message: None,
                    },
                )
                .await;
                push_downloader_complete(
                    state,
                    CompleteDownloaderRequest {
                        record_id: record_uuid,
                        target_path: vfs_target_path.clone(),
                        file_size: resp
                            .total_bytes
                            .or(resp.downloaded_bytes)
                            .map(|bytes| bytes.to_string()),
                    },
                )
                .await;

                // Dispatch file_scrape jobs so downloaded files get indexed into the
                // library (creates Movie/Episode records + media_file + ffprobe).
                if let Some(source_id) = download_source_id {
                    let lib_type = &lib_info.r#type;

                    if let Some(ref copy_result) = vfs_copy_result {
                        // VFS source: use uploaded VFS paths.
                        for file in &copy_result.files {
                            if !is_media_file(&file.vfs_path) {
                                continue;
                            }
                            let dir_path = file
                                .vfs_path
                                .rsplit_once('/')
                                .map(|(d, _)| d.to_string())
                                .unwrap_or_default();
                            match JobRepo::create_job_via_bus(
                                state,
                                "file_scrape",
                                json!({
                                    "filePath": file.vfs_path,
                                    "dirPath": dir_path,
                                    "fileSize": file.size,
                                    "videoId": lib_uuid.to_string(),
                                    "sourceId": source_id.to_string(),
                                    "libType": lib_type,
                                }),
                                None,
                                job_owner_user_id,
                            )
                            .await
                            {
                                Ok(job) => state.bus_notify_job(&job.into()),
                                Err(e) => warn!("Failed to dispatch file_scrape for {}: {e}", file.vfs_path),
                            }
                        }
                    } else {
                        // Local source: create media_file for immediate visibility,
                        // then dispatch file_scrape (which runs ffprobe inline).
                        for output in &resp.output_files {
                            if !is_media_file(&output.path) {
                                continue;
                            }
                            create_media_file_for_output(db, output, source_id, fs_driver_root.as_deref()).await;

                            let rel_path = to_relative_path(&output.path, fs_driver_root.as_deref());
                            let dir_path = rel_path
                                .rsplit_once('/')
                                .map(|(d, _)| d.to_string())
                                .unwrap_or_default();
                            match JobRepo::create_job_via_bus(
                                state,
                                "file_scrape",
                                json!({
                                    "filePath": rel_path,
                                    "dirPath": dir_path,
                                    "fileSize": output.size_bytes.unwrap_or(0),
                                    "videoId": lib_uuid.to_string(),
                                    "sourceId": source_id.to_string(),
                                    "libType": lib_type,
                                }),
                                None,
                                job_owner_user_id,
                            )
                            .await
                            {
                                Ok(job) => state.bus_notify_job(&job.into()),
                                Err(e) => warn!("Failed to dispatch file_scrape for {}: {e}", output.path),
                            }
                        }
                    }
                }

                return Ok(Some(json!({
                    "taskId": task_id,
                    "targetPath": vfs_target_path,
                    "manifestPath": null,
                })));
            }
            rust_online_media_ingest::models::TaskState::Failed
            | rust_online_media_ingest::models::TaskState::Cancelled => {
                let message = resp.error.unwrap_or_else(|| "在线媒体下载失败".into());
                error!(record_id, task_id, %message, "Online media task failed");
                append_download_log(state, &record_uuid, &task_id, "error", &message, None).await;
                update_record_failed(state, record_uuid, &message).await;
                return Err(message.into());
            }
            _ => {
                // Still running — continue polling.
            }
        }
    }
}

fn to_record_progress(progress: Option<f64>) -> f64 {
    match progress {
        Some(p) if !p.is_nan() => {
            let ratio = if p > 1.0 { p / 100.0 } else { p };
            ratio.clamp(0.0, 1.0)
        }
        _ => 0.0,
    }
}

fn stage_to_log_phase(stage: &str) -> &'static str {
    match stage {
        "preparing" | "queued" => "download-started",
        "analyzing" => "analyze",
        "packaging" => "manifest-import",
        "completed" => "completed",
        "failed" => "error",
        _ => "download-progress",
    }
}

fn stage_to_log_message(stage: &str, progress: Option<f64>) -> String {
    let pct = progress
        .map(|p| format!(" ({:.0}%)", p.clamp(0.0, 100.0)))
        .unwrap_or_default();
    match stage {
        "preparing" => "准备下载环境".into(),
        "queued" => "已加入下载队列".into(),
        "analyzing" => format!("分析媒体信息{pct}"),
        "downloading" => format!("正在下载{pct}"),
        "packaging" => format!("处理文件{pct}"),
        "completed" => "任务完成".into(),
        "failed" => "任务失败".into(),
        other => format!("{other}{pct}"),
    }
}

/// Info about a single file uploaded to VFS.
struct VfsCopiedFile {
    vfs_path: String,
    size: i64,
}

/// Result from copying staged files to VFS.
struct VfsCopyResult {
    /// Top-level VFS directory path (e.g. "/网片/Bilibili").
    target_path: String,
    /// Individual files uploaded, with their VFS paths and sizes.
    files: Vec<VfsCopiedFile>,
}

/// Copies all files from a local staging directory tree to a VFS target directory.
/// Returns the top-level VFS directory path and the list of uploaded files.
async fn copy_staged_to_vfs(
    sources: &crate::services::media::source::SourceRegistry,
    state: &Arc<AppState>,
    source_id: &str,
    local_stage_dir: &std::path::Path,
    vfs_target_root: &str,
    record_id: &Uuid,
    task_id: &str,
) -> Result<VfsCopyResult, String> {
    let vfs = sources
        .ensure_vfs(source_id)
        .await
        .map_err(|e| format!("VFS 不可用: {e}"))?;

    // Walk the local staging dir and collect all files.
    let mut stack = vec![local_stage_dir.to_path_buf()];
    let mut files: Vec<(std::path::PathBuf, String)> = Vec::new(); // (local_path, vfs_path)
    let mut uploaded_files: Vec<VfsCopiedFile> = Vec::new();

    // Determine the top-level subdirectory name inside staging (the organised dir).
    // e.g.  staging/{record}-vfs-target/Video Title [xxxx]/file.mp4
    //  → VFS: /网片/Video Title [xxxx]/file.mp4
    let vfs_root = vfs_target_root.trim_end_matches('/');

    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| format!("读取目录失败 {}: {e}", dir.display()))?;
        while let Some(entry) = entries.next_entry().await.map_err(|e| format!("遍历目录失败: {e}"))? {
            let ft = entry.file_type().await.map_err(|e| format!("获取文件类型失败: {e}"))?;
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                let local_path = entry.path();
                // Build relative path from staging dir root.
                let rel = local_path
                    .strip_prefix(local_stage_dir)
                    .map_err(|_| "路径计算失败".to_string())?;
                let vfs_path = format!("{vfs_root}/{}", rel.to_string_lossy().replace('\\', "/"));
                files.push((local_path, vfs_path));
            }
        }
    }

    if files.is_empty() {
        return Err("暂存目录为空，没有文件可上传".into());
    }

    // Determine the top-level organised directory VFS path (first path component below vfs_root).
    let first_vfs_path = &files[0].1;
    let rel_from_root = first_vfs_path
        .strip_prefix(&format!("{vfs_root}/"))
        .unwrap_or(first_vfs_path.as_str());
    let top_dir = rel_from_root
        .split('/')
        .next()
        .map_or_else(|| vfs_root.to_string(), |d| format!("{vfs_root}/{d}"));

    // Ensure directories exist and upload each file.
    let mut created_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (local_path, vfs_path) in &files {
        // Recursively ensure all path components exist between vfs_root and the file.
        // mkdir ignores "already exists" errors so existing dirs are safe to re-create.
        if let Some(parent) = std::path::Path::new(vfs_path).parent() {
            let parent_str = parent.to_string_lossy().into_owned();
            if created_dirs.insert(parent_str.clone()) {
                // Walk from vfs_root down to parent, creating each component.
                let components: Vec<_> = parent.components().collect();
                let mut curr = std::path::PathBuf::new();
                for component in &components {
                    curr.push(component);
                    let curr_str = curr.to_string_lossy();
                    if curr_str == "/" || curr_str.is_empty() {
                        continue;
                    }
                    // Ignore "already exists" errors but log other failures.
                    if let Err(e) = vfs.mkdir(std::path::Path::new(curr_str.as_ref())).await {
                        warn!("online_media_download: failed to mkdir {}: {e}", curr_str.as_ref());
                    }
                }
            }
        }

        // Stream-upload the file.
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(|e| format!("读取文件元数据失败: {e}"))?;
        let size = metadata.len();

        if vfs.has_put_stream(std::path::Path::new(vfs_path)).await {
            let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
            let local = local_path.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt as _;
                let Ok(mut file) = tokio::fs::File::open(&local).await else {
                    return;
                };
                let mut buf = vec![0u8; 256 * 1024];
                loop {
                    match file.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.send(buf[..n].to_vec()).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
            vfs.put_stream(std::path::Path::new(vfs_path), size, rx)
                .await
                .map_err(|e| format!("上传文件失败 {vfs_path}: {e}"))?;
        } else {
            let data = tokio::fs::read(local_path)
                .await
                .map_err(|e| format!("读取文件失败: {e}"))?;
            vfs.put(std::path::Path::new(vfs_path), data)
                .await
                .map_err(|e| format!("上传文件失败 {vfs_path}: {e}"))?;
        }

        append_download_log(
            state,
            record_id,
            task_id,
            "manifest-import",
            &format!("已上传: {vfs_path}"),
            None,
        )
        .await;

        uploaded_files.push(VfsCopiedFile {
            vfs_path: vfs_path.clone(),
            size: size as i64,
        });
    }

    Ok(VfsCopyResult {
        target_path: top_dir,
        files: uploaded_files,
    })
}

async fn update_record_failed(state: &Arc<AppState>, record_id: Uuid, message: &str) {
    // Use bus API to update status via host; no direct local shared-table status mutation
    push_downloader_status(
        state,
        UpdateDownloaderStatusRequest {
            record_id,
            status: Some("failed".into()),
            progress: None,
            downloaded_bytes: None,
            download_speed: Some(0),
            eta_seconds: None,
            thumbnail_url: None,
            error_message: Some(message.to_string()),
        },
    )
    .await;
}

const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mkv", "avi", "wmv", "flv", "mov", "webm", "ts", "m2ts", "mts", "mpg", "mpeg", "3gp", "rmvb", "rm",
    "mp3", "flac", "wav", "aac", "ogg", "opus", "m4a", "wma", "alac",
];

fn is_media_file(path: &str) -> bool {
    let ext = path.rsplit('.').next().map(str::to_ascii_lowercase).unwrap_or_default();
    MEDIA_EXTENSIONS.contains(&ext.as_str())
}

fn guess_mime(filename: &str) -> Option<String> {
    let ext = filename.rsplit('.').next()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "ts" | "m2ts" | "mts" => "video/mp2t",
        "mpg" | "mpeg" => "video/mpeg",
        "3gp" => "video/3gpp",
        "rmvb" | "rm" => "application/vnd.rn-realmedia-vbr",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "ogg" | "opus" => "audio/ogg",
        "m4a" => "audio/mp4",
        "wma" => "audio/x-ms-wma",
        "alac" => "audio/x-alac",
        _ => return None,
    };
    Some(mime.to_string())
}

/// Converts an absolute output file path to a VFS-relative path by stripping
/// the file system's driver root (e.g. `root_folder_path`).
fn to_relative_path(abs_path: &str, driver_root: Option<&str>) -> String {
    if let Some(root) = driver_root
        && abs_path.starts_with(root)
        && abs_path.len() > root.len()
    {
        let rel = &abs_path[root.len()..];
        if rel.starts_with('/') {
            return rel.to_string();
        }
    }
    abs_path.to_string()
}

/// Creates a `video_files` record for one downloaded output file. Returns the
/// newly created video file ID, or `None` if the file is not a media file or
/// the insert fails.
async fn create_media_file_for_output(
    db: &DatabaseConnection,
    output: &rust_online_media_ingest::models::OutputFile,
    source_id: Uuid,
    driver_root: Option<&str>,
) -> Option<Uuid> {
    if !is_media_file(&output.path) {
        return None;
    }

    let rel_path = to_relative_path(&output.path, driver_root);
    let filename = output.path.rsplit('/').next().unwrap_or(&output.path).to_string();
    let size = output.size_bytes.map(|s| s as i64);
    let mime = guess_mime(&filename);
    let video_file_id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();

    let model = video_files::ActiveModel {
        id: Set(video_file_id),
        source_id: Set(Some(source_id)),
        path: Set(rel_path.clone()),
        filename: Set(filename.clone()),
        size: Set(size),
        mime_type: Set(mime),
        duration: Set(None),
        checksum: Set(None),
        video_codec: Set(None),
        video_width: Set(None),
        video_height: Set(None),
        video_profile: Set(None),
        hdr_type: Set(None),
        video_streams: Set(None),
        audio_streams: Set(None),
        is_available: Set(true),
        scanned_at: Set(None),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        video_item_id: Set(None),
        episode_id: Set(None),
        ffprobe_raw: Set(None),
        iso_meta: Set(None),
    };

    match video_files::Entity::insert(model).exec(db).await {
        Ok(_) => {
            info!(
                %video_file_id,
                %filename,
                "Created video file record for online media download"
            );
            Some(video_file_id)
        }
        Err(e) => {
            error!(%filename, "Failed to create video file record: {e}");
            None
        }
    }
}
