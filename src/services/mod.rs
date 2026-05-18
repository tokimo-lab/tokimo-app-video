//! Phase 2A scaffold 阶段：filter-repo 出来的模块 + 待 cp 模块的 mod 声明。
//!
//! Sub-agent 后续需要：
//! - cp 主仓 `services/media/source/` 整目录到 `source/`
//! - cp 主仓 `services/storage/` 整目录到 `storage/`
//! - cp 主仓 `services/stream_session.rs` 到 `stream_session.rs`
//! - cp 主仓 `queue/handlers/common.rs` 到 `common.rs`
//! - cp 主仓 `queue/handlers/nfo_parser.rs` 到 `nfo_parser.rs`

pub mod app_sync;
pub mod scrape;

// 待 cp 模块（占位 mod 声明，sub-agent 接手）：
// pub mod source;
// pub mod storage;
// pub mod stream_session;
// pub mod common;
// pub mod nfo_parser;
