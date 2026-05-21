use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rust_client_api::downloaders::{
    qbittorrent::{QBittorrentClient, QBittorrentConfig},
    traits::{DownloadClient, TorrentInfo, TorrentState},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use tokimo_bus_client::BusClientBuilder;
use tokimo_bus_protocol::{BusError, HttpMethod, MethodDecl};
use uuid::Uuid;

use crate::AppState;
use crate::db::entities::{download_clients, download_records};
use crate::db::repos::download_record_repo::DownloadRecordRepo;

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordActionRequest {
    record_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnrichMetadataRequest {
    record_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnrichedMetadata {
    title: Option<String>,
    thumbnail_url: Option<String>,
    extra: JsonValue,
}

fn decode<T: for<'de> Deserialize<'de>>(raw: &[u8]) -> Result<T, BusError> {
    serde_json::from_slice(raw).map_err(|e| BusError::BadRequest(format!("json decode: {e}")))
}

fn unsupported(downloader_type: &str, op: &str) -> BusError {
    BusError::BadRequest(format!(
        "video downloader '{downloader_type}' does not support {op} in the current sidecar"
    ))
}

fn is_pt_downloader_type(downloader_type: &str) -> bool {
    downloader_type.starts_with("pt-")
}

fn qbt_status_for_update(torrent: &TorrentInfo) -> String {
    match &torrent.state {
        TorrentState::PausedDl | TorrentState::PausedUp => "paused".to_string(),
        TorrentState::QueuedDl | TorrentState::QueuedUp => "queued".to_string(),
        TorrentState::CheckingDl | TorrentState::CheckingUp => "checking".to_string(),
        TorrentState::StalledDl | TorrentState::StalledUp => "stalled".to_string(),
        TorrentState::Error | TorrentState::MissingFiles => "failed".to_string(),
        _ => "downloading".to_string(),
    }
}

fn qbt_live_metadata(torrent: &TorrentInfo) -> JsonValue {
    json!({
        "torrentHash": torrent.hash.clone(),
        "torrentState": torrent.state.clone(),
        "downloadSpeed": torrent.dl_speed,
        "uploadedBytes": torrent.uploaded,
        "downloadedBytes": torrent.downloaded,
        "progress": torrent.progress,
        "etaSeconds": torrent.eta,
        "size": torrent.size,
        "savePath": torrent.save_path.clone(),
        "category": torrent.category.clone(),
        "tags": torrent.tags.clone(),
        "tracker": torrent.tracker.clone(),
    })
}

fn qbt_is_complete(torrent: &TorrentInfo) -> bool {
    torrent.progress >= 0.999 || matches!(&torrent.state, TorrentState::Seeding | TorrentState::Uploading)
}

async fn load_qbt_client(ctx: &AppState, download_client_id: Uuid) -> Result<Arc<QBittorrentClient>, BusError> {
    let model = download_clients::Entity::find_by_id(download_client_id)
        .one(&ctx.db)
        .await
        .map_err(|e| BusError::Internal(e.to_string()))?
        .ok_or_else(|| BusError::BadRequest(format!("download client not found: {download_client_id}")))?;

    Ok(Arc::new(QBittorrentClient::new(QBittorrentConfig {
        url: model.url,
        username: model.username.unwrap_or_default(),
        password: model.password.unwrap_or_default(),
    })))
}

async fn load_record_qbt_client(
    ctx: &AppState,
    record: &download_records::Model,
) -> Result<(Arc<QBittorrentClient>, String), BusError> {
    let download_client_id = record
        .download_client_id
        .ok_or_else(|| BusError::BadRequest(format!("download client is missing for record {}", record.id)))?;
    let torrent_hash = record
        .torrent_hash
        .clone()
        .ok_or_else(|| BusError::BadRequest(format!("torrent hash is missing for record {}", record.id)))?;
    let client = load_qbt_client(ctx, download_client_id).await?;
    Ok((client, torrent_hash))
}

async fn dispatch_action(ctx: Arc<AppState>, op: &'static str, record_id: Uuid) -> Result<(), BusError> {
    let record = DownloadRecordRepo::get_model_by_id(&ctx.db, record_id)
        .await
        .map_err(|e| BusError::Internal(e.to_string()))?
        .ok_or_else(|| BusError::BadRequest(format!("download record not found: {record_id}")))?;

    match record.downloader_type.as_str() {
        "yt-dlp" => match op {
            "cancel" => {
                let task_id = ctx.download_tasks.lock().await.get(&record_id).cloned();
                let Some(task_id) = task_id else {
                    return Err(BusError::BadRequest(format!(
                        "yt-dlp task for record {record_id} is not active in this sidecar"
                    )));
                };
                if ctx.online_media.tasks.request_cancel(&task_id).await {
                    Ok(())
                } else {
                    Err(BusError::BadRequest(format!("yt-dlp task not found: {task_id}")))
                }
            }
            "pause" | "resume" => Err(unsupported("yt-dlp", op)),
            _ => Err(BusError::BadRequest(format!("unknown downloader op: {op}"))),
        },
        "pt-qbittorrent" => {
            let (client, torrent_hash) = load_record_qbt_client(&ctx, &record).await?;
            let hashes = [torrent_hash.as_str()];
            match op {
                "pause" => client
                    .pause_torrents(&hashes)
                    .await
                    .map_err(|e| BusError::Internal(e.to_string())),
                "resume" => client
                    .resume_torrents(&hashes)
                    .await
                    .map_err(|e| BusError::Internal(e.to_string())),
                "cancel" => client
                    .delete_torrents(&hashes, false)
                    .await
                    .map_err(|e| BusError::Internal(e.to_string())),
                _ => Err(BusError::BadRequest(format!("unknown downloader op: {op}"))),
            }
        }
        other => Err(BusError::BadRequest(format!("unknown video downloader type: {other}"))),
    }
}

async fn enrich_metadata(
    ctx: Arc<AppState>,
    record_ids: Vec<Uuid>,
) -> Result<HashMap<String, EnrichedMetadata>, BusError> {
    let mut result = HashMap::new();
    for record_id in record_ids {
        let Some(record) = DownloadRecordRepo::get_model_by_id(&ctx.db, record_id)
            .await
            .map_err(|e| BusError::Internal(e.to_string()))?
        else {
            continue;
        };
        let mut extra = record.app_metadata.clone().unwrap_or_else(|| json!({}));
        if !extra.is_object() {
            extra = json!({});
        }
        if is_pt_downloader_type(&record.downloader_type)
            && let Ok((client, torrent_hash)) = load_record_qbt_client(&ctx, &record).await
            && let Ok(Some(torrent)) = client.get_torrent(&torrent_hash).await
        {
            let live = qbt_live_metadata(&torrent);
            if let Some(extra_obj) = extra.as_object_mut()
                && let Some(live_obj) = live.as_object()
            {
                for (key, value) in live_obj {
                    extra_obj.insert(key.clone(), value.clone());
                }
            }
        }
        let title = extra
            .get("mediaTitle")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Some(record.title.clone()));
        let thumbnail_url = record
            .thumbnail_url
            .clone()
            .or_else(|| extra.get("thumbnailUrl").and_then(|v| v.as_str()).map(String::from));
        result.insert(
            record_id.to_string(),
            EnrichedMetadata {
                title,
                thumbnail_url,
                extra,
            },
        );
    }
    Ok(result)
}

async fn sync_pt_record_status(ctx: Arc<AppState>) -> Result<(), BusError> {
    let records = download_records::Entity::find()
        .filter(download_records::Column::AppId.eq("video"))
        .filter(download_records::Column::DownloaderType.like("pt-%"))
        .filter(download_records::Column::Status.is_not_in(["completed", "failed", "cancelled", "organized"]))
        .all(&ctx.db)
        .await
        .map_err(|e| BusError::Internal(e.to_string()))?;

    let mut clients: HashMap<Uuid, Arc<QBittorrentClient>> = HashMap::new();
    for record in records {
        if !record.downloader_type.eq("pt-qbittorrent") {
            continue;
        }
        let Some(download_client_id) = record.download_client_id else {
            continue;
        };
        let Some(torrent_hash) = record.torrent_hash.as_deref() else {
            continue;
        };

        let client = if let Some(client) = clients.get(&download_client_id) {
            Arc::clone(client)
        } else {
            let client = load_qbt_client(&ctx, download_client_id).await?;
            clients.insert(download_client_id, Arc::clone(&client));
            client
        };

        let Some(torrent) = client
            .get_torrent(torrent_hash)
            .await
            .map_err(|e| BusError::Internal(e.to_string()))?
        else {
            continue;
        };

        if qbt_is_complete(&torrent) {
            if let Some(bus) = ctx.bus_client.get() {
                let request = crate::bus_clients::downloader::CompleteDownloaderRequest {
                    record_id: record.id,
                    target_path: Some(torrent.save_path.clone()),
                    file_size: Some(torrent.size.to_string()),
                };
                let _ = crate::bus_clients::downloader::complete(bus, &request).await;
            }
            continue;
        }

        if let Some(bus) = ctx.bus_client.get() {
            let request = crate::bus_clients::downloader::UpdateDownloaderStatusRequest {
                record_id: record.id,
                status: Some(qbt_status_for_update(&torrent)),
                progress: Some(torrent.progress),
                downloaded_bytes: i64::try_from(torrent.downloaded).ok(),
                download_speed: i64::try_from(torrent.dl_speed).ok(),
                eta_seconds: torrent.eta.and_then(|eta| i32::try_from(eta).ok()),
                thumbnail_url: None,
                error_message: None,
            };
            let _ = crate::bus_clients::downloader::update_status(bus, &request).await;
        }
    }

    Ok(())
}

pub fn spawn_pt_status_sync(ctx: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(error) = sync_pt_record_status(Arc::clone(&ctx)).await {
                tracing::warn!(%error, "video: PT status sync failed");
            }
        }
    });
}

pub fn register(builder: BusClientBuilder, ctx: Arc<AppState>) -> BusClientBuilder {
    let ctx_pause = ctx.clone();
    let ctx_resume = ctx.clone();
    let ctx_cancel = ctx.clone();
    let ctx_enrich = ctx;

    builder
        .method(decl("downloader.pause", "Pause a video-owned download record"))
        .on_invoke("downloader.pause", move |req| {
            let ctx = ctx_pause.clone();
            async move {
                let input: RecordActionRequest = decode(&req.payload)?;
                dispatch_action(ctx, "pause", input.record_id)
                    .await
                    .map(|_| b"{}".to_vec())
            }
        })
        .method(decl("downloader.resume", "Resume a video-owned download record"))
        .on_invoke("downloader.resume", move |req| {
            let ctx = ctx_resume.clone();
            async move {
                let input: RecordActionRequest = decode(&req.payload)?;
                dispatch_action(ctx, "resume", input.record_id)
                    .await
                    .map(|_| b"{}".to_vec())
            }
        })
        .method(decl("downloader.cancel", "Cancel a video-owned download record"))
        .on_invoke("downloader.cancel", move |req| {
            let ctx = ctx_cancel.clone();
            async move {
                let input: RecordActionRequest = decode(&req.payload)?;
                dispatch_action(ctx, "cancel", input.record_id)
                    .await
                    .map(|_| b"{}".to_vec())
            }
        })
        .method(decl(
            "downloader.enrich_metadata",
            "Return video metadata for download records",
        ))
        .on_invoke("downloader.enrich_metadata", move |req| {
            let ctx = ctx_enrich.clone();
            async move {
                let input: EnrichMetadataRequest = decode(&req.payload)?;
                let enriched = enrich_metadata(ctx, input.record_ids).await?;
                serde_json::to_vec(&enriched).map_err(|e| BusError::Internal(e.to_string()))
            }
        })
}
