//! AppCtx — video binary 的共享应用状态。
//!
//! Phase 2A scaffold: 目前只含 db + client，**13 个真实字段由 Phase 2A 后续 sub-agent
//! 在 cp 完 `services/source` / `services/storage` / `services/stream_session` 模块后扩展**。
//!
//! 字段清单（见 plan.md §6 Phase 2A）：
//! - `sources: Arc<services::source::SourceRegistry>`             (cp 主仓 services/media/source/)
//! - `storage: Arc<dyn services::storage::StorageProvider>`       (cp 主仓 services/storage/)
//! - `hls_manager: Arc<HlsSessionManager>`                        (tokimo-package-hls)
//! - `stream_sessions: services::stream_session::StreamSessionManager` (cp 主仓单文件)
//! - `subtitle_cache: SubtitleCache`                              (tokimo-package-subtitle)
//! - `tap_registry: TapRegistry`                                  (tokimo-package-subtitle)
//! - `online_media: Arc<rust_online_media_ingest::AppState>`
//! - `subtitle_aggregator: Arc<subtitle_aggregator::aggregator::SubtitleAggregator>`
//! - `http_client: reqwest::Client`
//! - `event_tx: queue::AppEventSender`                            (video 自建 mpsc channel)
//! - `download_log_bus: Arc<services::log_bus::LogBus>`           (bus call 主仓 stub)
//! - `tv_show_creation_locks: Arc<TokioMutex<HashMap<String, Arc<TokioMutex<()>>>>>`
//! - `screenshot_semaphore: Arc<Semaphore>`

use std::sync::{Arc, OnceLock};

use sea_orm::DatabaseConnection;
use tokimo_bus_client::BusClient;

pub struct AppCtx {
    pub db: DatabaseConnection,
    /// 延迟绑定的 BusClient（构造后 client_slot.set 注入）—— 给 handlers / repos
    /// 做 cross-app bus call（如 `vfs.get_driver_config` / `jobs.create` / `playback_state.set`）。
    pub client: Arc<OnceLock<Arc<BusClient>>>,
    // TODO(Phase 2A sub-agent): 在 cp 完 services modules 后追加 13 字段
}

impl AppCtx {
    pub async fn new(
        db: DatabaseConnection,
        client: Arc<OnceLock<Arc<BusClient>>>,
    ) -> anyhow::Result<Self> {
        Ok(Self { db, client })
    }

    /// 拿 BusClient 引用，未注入时返回 None（启动早期可能未绑定）。
    pub fn bus(&self) -> Option<Arc<BusClient>> {
        self.client.get().cloned()
    }
}
