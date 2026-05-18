//! Phase 2A scaffold: 增补 filter-repo 漏的 online_media_download 声明。

pub mod online_media_download;
pub mod tmdb_person_scrape;
pub mod tv_scrape;
pub mod video_item_scrape;

// TODO(Phase 2A sub-agent): video 仓内自建 AppEventSender mpsc channel
// 主仓的 queue::AppEventSender 进程外不可达，video 自己 consume 即可
// pub type AppEventSender = tokio::sync::mpsc::UnboundedSender<AppEvent>;
