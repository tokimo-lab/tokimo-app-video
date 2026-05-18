use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use sea_orm::DatabaseConnection;
use tokio::sync::{Mutex as TokioMutex, Semaphore, broadcast};

use crate::queue::AppEvent;
use crate::services::downloads::log_bus::LogBus;
use crate::services::media::source::SourceRegistry;
use crate::services::storage::StorageProvider;
use crate::services::stream_session::StreamSessionManager;

pub struct AppCtx {
    pub db: DatabaseConnection,
    pub sources: Arc<SourceRegistry>,
    pub storage: Arc<dyn StorageProvider>,
    pub http_client: reqwest::Client,
    pub hls_manager: Arc<tokimo_package_hls::HlsSessionManager>,
    pub subtitle_cache: tokimo_package_subtitle::cache::SubtitleCache,
    pub tap_registry: tokimo_package_subtitle::cache::TapRegistry,
    pub stream_sessions: StreamSessionManager,
    pub subtitle_aggregator: Arc<subtitle_aggregator::aggregator::SubtitleAggregator>,
    pub tv_show_creation_locks: Arc<TokioMutex<HashMap<String, Arc<TokioMutex<()>>>>>,
    pub event_tx: broadcast::Sender<AppEvent>,
    pub download_log_bus: Arc<LogBus>,
    pub online_media: Arc<rust_online_media_ingest::AppState>,
    pub screenshot_semaphore: Arc<Semaphore>,
    pub bus_client: Arc<OnceLock<Arc<tokimo_bus_client::BusClient>>>,
}

impl AppCtx {
    pub async fn new(
        db: sea_orm::DatabaseConnection,
        client_slot: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<tokimo_bus_client::BusClient>>>,
    ) -> anyhow::Result<Self> {
        let (event_tx, _) = broadcast::channel(256);
        let sources = Arc::new(crate::services::source::SourceRegistry::new(db.clone()));
        let data_local_path = std::env::var("DATA_LOCAL_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("./data/local"));
        let storage = crate::services::storage::create_storage_from_env(&data_local_path);
        let http_client = reqwest::Client::new();
        let hls_manager = Arc::new(tokimo_package_hls::HlsSessionManager::new());
        let subtitle_aggregator = Arc::new(subtitle_aggregator::aggregator::SubtitleAggregator::default());
        let online_media = Arc::new(rust_online_media_ingest::AppState {
            staging_root: std::path::PathBuf::from("./data/online_media"),
            tasks: Arc::new(rust_online_media_ingest::task_manager::TaskManager::new()),
        });
        let screenshot_semaphore = Arc::new(Semaphore::new(4));

        Ok(Self {
            db,
            sources,
            storage,
            http_client,
            hls_manager,
            subtitle_cache: Default::default(),
            tap_registry: Default::default(),
            stream_sessions: crate::services::stream_session::StreamSessionManager::new(),
            subtitle_aggregator,
            tv_show_creation_locks: Default::default(),
            event_tx,
            download_log_bus: Arc::new(crate::services::downloads::log_bus::LogBus::new()),
            online_media,
            screenshot_semaphore,
            bus_client: client_slot,
        })
    }
}
