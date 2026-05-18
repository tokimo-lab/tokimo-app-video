use std::collections::HashMap;
use std::sync::Arc;
use sea_orm::DatabaseConnection;
use tokio::sync::{Mutex as TokioMutex, broadcast};

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
}
