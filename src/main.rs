//! Video app — 内嵌 axum + UDS sidecar 形态（参考 helloworld 范式）。
//!
//! 启动流程：
//! 1. 连接 broker（健康检查 + cross-app bus call）
//! 2. 起 axum router 监听 `<runtime_dir>/apps/video.sock`
//! 3. 报 sock 给 broker（`data_plane_socket`）
//! 4. 主 server 把 `/api/apps/video/<rest>` 全部反代到本 sock 的 `/<rest>`

mod app_server;
mod assets;
mod bus_clients;
mod bus_services;
mod cli;
mod config;
mod db;
mod error;
mod handlers;
mod queue;
mod router;
mod services;
mod state;
mod apps;

pub use state::AppCtx as AppState;

use std::sync::{Arc, OnceLock};

use clap::{Parser, Subcommand};
use tokimo_bus_cli::TokimoAuthArgs;
use tokimo_bus_client::{BusClient, ClientConfig};
use tracing::{error, info};

use crate::state::AppCtx;

#[derive(Parser, Debug)]
#[command(
    name = "tokimo-app-video",
    about = "Tokimo Video — CLI / sidecar binary",
    long_about = "Tokimo Video CLI — 通过 Tokimo 主 server 调用 video app。\n\n前置条件：\n1. 启动 Tokimo 主 server (默认 http://localhost:5678)\n2. 浏览器登录后，「设置 → API Keys」创建 token (mm_xxx)\n3. 通过 --tokimo-token 或 TOKIMO_TOKEN env 传入",
    term_width = 100
)]
struct Cli {
    #[command(flatten)]
    auth: TokimoAuthArgs,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 列出所有视频库
    Libraries,
    /// 打印版本
    Version,
    /// 单机模式启动 HTTP server（不注册 bus，用于开发测试）
    Standalone {
        /// TCP 监听端口
        #[arg(long, default_value = "5680")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Cli { auth, command } = Cli::parse();

    match command {
        None if std::env::var_os("TOKIMO_BUS_SOCKET").is_some() => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info,tokimo_bus_client=info,tokimo_app_video=debug".into()),
                )
                .init();
            if let Err(error) = run_server().await {
                error!(%error, "video: fatal");
                std::process::exit(1);
            }
        }
        None => {
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            tokimo_bus_cli::print_help_unified(&mut cmd);
            std::process::exit(0);
        }
        Some(cmd) => {
            let result = match cmd {
                Command::Libraries => cli::run_libraries(auth).await,
                Command::Version => {
                    println!("tokimo-app-video {}", env!("CARGO_PKG_VERSION"));
                    Ok(())
                }
                Command::Standalone { port } => {
                    tracing_subscriber::fmt()
                        .with_env_filter(
                            tracing_subscriber::EnvFilter::try_from_default_env()
                                .unwrap_or_else(|_| "info,tokimo_app_video=debug".into()),
                        )
                        .init();
                    run_standalone(port).await
                }
            };
            if let Err(error) = result {
                eprintln!("Error: {error:#}");
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

async fn run_server() -> anyhow::Result<()> {
    let cfg = ClientConfig::from_env().map_err(|e| anyhow::anyhow!("ClientConfig: {e}"))?;
    info!(endpoint = ?cfg.endpoint, "video: connecting to broker");

    let db = db::init_pool().await?;
    db::init_schema(&db).await?;
    info!("video: db ready");

    let client_slot: Arc<OnceLock<Arc<BusClient>>> = Arc::new(OnceLock::new());
    let ctx = AppCtx::new(db, Arc::clone(&client_slot)).await?;
    let ctx = Arc::new(ctx);

    let app_socket = app_server::spawn("video", Arc::clone(&ctx))
        .await
        .map_err(|e| anyhow::anyhow!("app_server spawn: {e}"))?;

    let client = bus_services::video_jobs::register(
        BusClient::builder(cfg)
            .service("video", env!("CARGO_PKG_VERSION"))
            .data_plane(app_socket),
        Arc::clone(&ctx),
    )
    .build()
    .await
    .map_err(|e| anyhow::anyhow!("bus build: {e}"))?;
    client_slot
        .set(Arc::clone(&client))
        .map_err(|_| anyhow::anyhow!("client_slot already set"))?;

    info!("video: registered with broker");

    let shutdown = {
        let client = Arc::clone(&client);
        tokio::spawn(async move { client.run_until_shutdown().await })
    };

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("video: SIGINT received");
            client.shutdown();
        }
        _ = shutdown => info!("video: broker sent Shutdown"),
    }

    Ok(())
}

async fn run_standalone(port: u16) -> anyhow::Result<()> {
    let db = db::init_pool().await?;
    db::init_schema(&db).await?;
    info!("video: db ready");

    let client_slot: Arc<OnceLock<Arc<BusClient>>> = Arc::new(OnceLock::new());
    let ctx = AppCtx::new(db, Arc::clone(&client_slot)).await?;
    let ctx = Arc::new(ctx);

    app_server::spawn_tcp(port, ctx).await?;
    info!(port, "video: standalone server started — http://127.0.0.1:{port}/health");

    tokio::signal::ctrl_c().await?;
    info!("video: SIGINT received");
    Ok(())
}
