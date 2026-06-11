//! CLI subcommands stub — Phase 2A: not yet implemented; all commands return unimplemented errors.
//!
//! Plan: in the future, call video.* endpoints exposed by the main server via bus call
//! (e.g. `video.libraries.list`), so agents / scripts can fetch data via
//! `tokimo-app-video libraries --tokimo-token mm_xxx`.

use tokimo_bus_cli::TokimoAuthArgs;

pub async fn run_libraries(_auth: TokimoAuthArgs) -> anyhow::Result<()> {
    anyhow::bail!("CLI not yet implemented (Phase 2A scaffold). Planned: bus call video.libraries.list")
}
