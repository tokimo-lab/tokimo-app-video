//! 内嵌 axum HTTP server，监听本地 socket（正常模式）或 TCP（standalone 模式）。
//!
//! 路由 = filter-repo 出来的 `crate::router::build_video_app_routes()` 提供的全部
//! `/api/apps/video/*` 树（去掉 prefix），加上 `/assets/{*path}` 静态资源 + `/health`。
//!
//! server 端 `/api/apps/video/<rest>` 反代到本 sock 的 `/<rest>`。

use std::sync::Arc;

use axum::{Router, routing::get};
use tokimo_bus_protocol::{BusListener, DataPlaneSocket};
use tracing::{error, info};

use crate::{assets, router, state::AppCtx};

pub async fn spawn(service: &str, ctx: Arc<AppCtx>) -> anyhow::Result<DataPlaneSocket> {
    let (listener, socket) = BusListener::bind_for_app(service)?;
    info!(?socket, "video: app server listening");

    let app = build_router(ctx);

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!(error = %e, "video: app server stopped");
        }
    });

    Ok(socket)
}

/// Standalone TCP mode — 不注册 bus，直接监听 TCP，用于开发测试。
pub async fn spawn_tcp(port: u16, ctx: Arc<AppCtx>) -> anyhow::Result<()> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "video: standalone TCP server listening");

    let app = build_router(ctx);

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!(error = %e, "video: standalone server stopped");
        }
    });

    Ok(())
}

fn build_router(ctx: Arc<AppCtx>) -> Router {
    router::build_video_app_routes()
        .merge(tokimo_jellyfin_api::build_jellyfin_routes::<crate::AppState>())
        .route("/assets/{*path}", get(assets::serve))
        .route("/health", get(health))
        .with_state(ctx)
}

async fn health() -> &'static str {
    "ok"
}
