//! CLI subcommands stub —— Phase 2A 暂不实现，所有命令返回 unimplemented 错误。
//!
//! 计划：未来通过 bus call 调主 server 暴露的 video.* endpoint（如 `video.libraries.list`），
//! 让 agent / 脚本能通过 `tokimo-app-video libraries --tokimo-token mm_xxx` 拿数据。

use tokimo_bus_cli::TokimoAuthArgs;

pub async fn run_libraries(_auth: TokimoAuthArgs) -> anyhow::Result<()> {
    anyhow::bail!("CLI not yet implemented (Phase 2A scaffold). 计划通过 bus call video.libraries.list 实现")
}
