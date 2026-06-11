#![allow(dead_code, deprecated)]

//! Video app — 内嵌 axum + UDS sidecar 形态（参考 helloworld 范式）。
//!
//! 启动流程：
//! 1. 连接 broker（健康检查 + cross-app bus call）
//! 2. 起 axum router 监听 `<runtime_dir>/apps/video.sock`
//! 3. 报 sock 给 broker（`data_plane_socket`）
//! 4. 主 server 把 `/api/apps/video/<rest>` 全部反代到本 sock 的 `/<rest>`

mod app_server;
mod apps;
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

pub(crate) const MANIFEST: &str = include_str!("../tokimo-app.toml");

pub use state::AppCtx as AppState;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use clap::{Parser, Subcommand};
use sea_orm::ConnectionTrait;
use tokimo_bus_cli::TokimoAuthArgs;
use tokimo_bus_client::{BusClient, ClientConfig};
use tracing::{error, info, warn};

use crate::state::AppCtx;

fn data_local_path() -> PathBuf {
    std::env::var("DATA_LOCAL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./.data/local"))
}

fn video_ytdlp_root() -> PathBuf {
    data_local_path().join("apps/video/bin/yt-dlp")
}

async fn init_ytdlp_root() -> anyhow::Result<PathBuf> {
    let root = video_ytdlp_root();
    tokimo_media_ingest::tooling::set_ytdlp_root_override(root.clone()).map_err(|root| {
        anyhow::anyhow!(
            "yt-dlp root override already initialized before video startup: {}",
            root.display()
        )
    })?;
    tokimo_media_ingest::tooling::ensure_ytdlp_available_at(&root).await;
    Ok(root)
}

#[derive(Parser, Debug)]
#[command(
    name = "tokimo-app-video",
    about = "Tokimo Video — CLI / sidecar binary",
    long_about = "Tokimo Video CLI — call the video app via the Tokimo main server.",
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
    /// List all video libraries
    Libraries,
    /// Print version
    Version,
    /// Start standalone HTTP server (no bus registration, for dev testing)
    Standalone {
        /// TCP listen port
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

    // Reset any sync statuses stuck at "syncing" from a previous crash
    db.execute_unprepared("UPDATE video.videos SET sync_status = 'idle' WHERE sync_status = 'syncing'")
        .await?;
    info!("video: db ready");

    let ytdlp_root = init_ytdlp_root().await?;
    let client_slot: Arc<OnceLock<Arc<BusClient>>> = Arc::new(OnceLock::new());
    let storage_slot: Arc<OnceLock<Arc<dyn tokimo_package_storage::StorageProvider>>> = Arc::new(OnceLock::new());
    storage_slot
        .set(Arc::new(
            tokimo_package_storage::OpendalStorageProvider::new(&data_local_path().join("storage"))
                .expect("storage init"),
        ))
        .map_err(|_| anyhow::anyhow!("storage_slot already set"))?;
    let ctx = AppCtx::new(db, Arc::clone(&client_slot), ytdlp_root, Arc::clone(&storage_slot)).await?;
    let ctx = Arc::new(ctx);

    let app_socket = app_server::spawn("video", Arc::clone(&ctx))
        .await
        .map_err(|e| anyhow::anyhow!("app_server spawn: {e}"))?;

    let builder = BusClient::builder(cfg)
        .service("video", env!("CARGO_PKG_VERSION"))
        .data_plane(app_socket);
    let builder = bus_services::video_jobs::register(builder, Arc::clone(&ctx));
    let builder = bus_services::downloader::register(builder, Arc::clone(&ctx));
    let client = builder.build().await.map_err(|e| anyhow::anyhow!("bus build: {e}"))?;
    client_slot
        .set(Arc::clone(&client))
        .map_err(|_| anyhow::anyhow!("client_slot already set"))?;

    // Register job handlers with the main server (appId inferred from bus caller).
    bus_clients::jobs::register_handler(&client, "file_scrape", "dispatch_file_scrape").await?;
    bus_clients::jobs::register_handler(&client, "tv_scrape", "dispatch_tv_scrape").await?;
    bus_clients::jobs::register_handler(&client, "movie_scrape", "dispatch_movie_scrape").await?;
    bus_clients::jobs::register_handler(&client, "tmdb_person_scrape", "dispatch_tmdb_person_scrape").await?;
    bus_clients::jobs::register_handler(&client, "online_media_ingest", "dispatch_online_video_download").await?;

    if let Err(error) = crate::bus_clients::downloader::register_downloaders(&client).await {
        warn!(%error, "video: failed to register downloader SDK with host");
    }
    bus_services::downloader::spawn_pt_status_sync(Arc::clone(&ctx));

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

    let ytdlp_root = init_ytdlp_root().await?;
    let client_slot: Arc<OnceLock<Arc<BusClient>>> = Arc::new(OnceLock::new());
    // Standalone mode: use local filesystem storage directly
    let data_local_path = data_local_path();
    let storage_slot: Arc<OnceLock<Arc<dyn tokimo_package_storage::StorageProvider>>> = Arc::new(OnceLock::new());
    storage_slot
        .set(Arc::new(
            tokimo_package_storage::OpendalStorageProvider::new(&data_local_path.join("storage"))
                .expect("storage init"),
        ))
        .map_err(|_| anyhow::anyhow!("storage_slot already set"))?;
    let ctx = AppCtx::new(db, Arc::clone(&client_slot), ytdlp_root, storage_slot).await?;
    let ctx = Arc::new(ctx);

    app_server::spawn_tcp(port, ctx).await?;
    info!(
        port,
        "video: standalone server started — http://127.0.0.1:{port}/health"
    );

    tokio::signal::ctrl_c().await?;
    info!("video: SIGINT received");
    Ok(())
}
