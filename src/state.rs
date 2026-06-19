use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::{Mutex as TokioMutex, Semaphore, broadcast};
use tracing::warn;

use crate::apps::media_organize::services::OrganizeSession;
use crate::db::models::job::JobOutput;
use crate::queue::AppEvent;
use crate::services::downloads::log_bus::LogBus;
use crate::services::media::source::SourceRegistry;
use crate::services::stream_session::StreamSessionManager;

pub struct AppCtx {
    pub db: DatabaseConnection,
    pub sources: Arc<SourceRegistry>,
    pub storage: Arc<OnceLock<Arc<dyn tokimo_package_storage::StorageProvider>>>,
    pub http_client: reqwest::Client,
    pub image_proxy_key: String,
    pub hls_manager: Arc<tokimo_package_hls::HlsSessionManager>,
    pub subtitle_cache: tokimo_package_subtitle::cache::SubtitleCache,
    pub tap_registry: tokimo_package_subtitle::cache::TapRegistry,
    pub stream_sessions: StreamSessionManager,
    pub tokimo_subtitle_search: Arc<tokimo_subtitle_search::aggregator::SubtitleAggregator>,
    pub tv_show_creation_locks: Arc<TokioMutex<HashMap<String, Arc<TokioMutex<()>>>>>,
    pub event_tx: broadcast::Sender<AppEvent>,
    pub download_log_bus: Arc<LogBus>,
    pub online_media: Arc<tokimo_media_ingest::AppState>,
    pub download_tasks: Arc<TokioMutex<HashMap<uuid::Uuid, String>>>,
    pub ytdlp_root: PathBuf,
    pub screenshot_semaphore: Arc<Semaphore>,
    pub organize_session: Arc<tokio::sync::RwLock<Option<OrganizeSession>>>,
    pub active_subscription_runs: Arc<RwLock<HashMap<String, String>>>,
    pub bus_client: Arc<OnceLock<Arc<tokimo_bus_client::BusClient>>>,
    pub auth_client: Arc<crate::bus_clients::auth::AuthClient>,
}

impl AppCtx {
    pub async fn new(
        db: sea_orm::DatabaseConnection,
        client_slot: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<tokimo_bus_client::BusClient>>>,
        ytdlp_root: PathBuf,
        storage_slot: Arc<OnceLock<Arc<dyn tokimo_package_storage::StorageProvider>>>,
    ) -> anyhow::Result<Self> {
        let (event_tx, _) = broadcast::channel(256);
        let sources = Arc::new(crate::services::source::SourceRegistry::new(
            db.clone(),
            Arc::clone(&client_slot),
        ));
        let data_local_path = std::env::var("TOKIMO_DATA_LOCAL_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("./.data/local"));
        let http_client = reqwest::Client::new();
        let image_proxy_key = hex::encode(rand::random::<[u8; 32]>());
        let hls_manager = Arc::new(tokimo_package_hls::HlsSessionManager::new());
        let tokimo_subtitle_search = Arc::new(tokimo_subtitle_search::aggregator::SubtitleAggregator::default());
        let online_media = Arc::new(tokimo_media_ingest::AppState {
            staging_root: data_local_path.join("online_media"),
            tasks: Arc::new(tokimo_media_ingest::task_manager::TaskManager::new()),
        });
        let screenshot_semaphore = Arc::new(Semaphore::new(4));

        Ok(Self {
            db,
            sources,
            storage: storage_slot,
            http_client,
            image_proxy_key,
            hls_manager,
            subtitle_cache: Default::default(),
            tap_registry: Default::default(),
            stream_sessions: crate::services::stream_session::StreamSessionManager::new(),
            tokimo_subtitle_search,
            tv_show_creation_locks: Default::default(),
            event_tx,
            download_log_bus: Arc::new(crate::services::downloads::log_bus::LogBus::new()),
            online_media,
            download_tasks: Default::default(),
            ytdlp_root,
            screenshot_semaphore,
            organize_session: Arc::new(tokio::sync::RwLock::new(None)),
            active_subscription_runs: Arc::new(RwLock::new(HashMap::new())),
            auth_client: Arc::new(crate::bus_clients::auth::AuthClient::new(Arc::clone(&client_slot))),
            bus_client: client_slot,
        })
    }

    pub fn image_proxy_key(&self) -> &str {
        &self.image_proxy_key
    }

    /// 获取 storage provider（必须在 bus client 就绪后调用）。
    pub fn storage(&self) -> &Arc<dyn tokimo_package_storage::StorageProvider> {
        self.storage
            .get()
            .expect("storage not initialized — bus client must be connected first")
    }
}

impl AppCtx {
    /// Publish a job snapshot to the main server's `task_queue` service via
    /// the bus so it appears in the global task-queue UI.
    ///
    /// Call this after any job status change in addition to the local
    /// `event_tx.send(AppEvent::JobUpdate { ... })`.
    ///
    /// No-op when the bus client is not yet initialised (e.g. standalone mode).
    pub fn bus_notify_job(&self, job: &JobOutput) {
        let Some(client) = self.bus_client.get() else { return };
        // Build the UpsertJobReq payload expected by task_queue service.
        let Ok(payload) = serde_json::to_vec(&serde_json::json!({
            "jobId":    job.id,
            "appId":    "video",
            "userId":   job.user_id,
            "title":    job.r#type,
            "status":   job.status,
            "progress": job.progress,
            "metadata": {},
            "parentJobId": job.parent_job_id,
            "startedAt": job.started_at,
            "updatedAt": job.updated_at,
            "finishedAt": job.completed_at,
        })) else {
            return;
        };
        let client = Arc::clone(client);
        tokio::spawn(async move {
            if let Err(e) = client
                .invoke("task_queue", "upsert_job", payload, client.auto_caller("video"))
                .await
            {
                warn!(err = %e, "bus_notify_job: failed to upsert job on bus");
            }
        });
    }
}
