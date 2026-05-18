//! SeaORM entities — Phase 2A scaffold 阶段为空，sub-agent 后续 cp 主仓 video 相关 entities。
//!
//! 需要 cp 的 entities（来自 `packages/rust-server/src/db/entities/`）：
//! - video_items / tv_shows / seasons / episodes / video_files
//! - video_persons / tv_persons / video_cast / tv_season_cast
//! - video_collections / playback_sessions / watch_histories
//! - subtitles / chapters / video_genres / tv_show_genres
//! - scrape_settings / scrape_tasks / scrape_queue_*
//! - vfs (read-only mirror — video binary 直读 public.vfs，Phase 2B 改 bus call)
