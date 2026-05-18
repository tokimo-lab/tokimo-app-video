//! video binary DB 入口。
//!
//! schema 策略（plan.md §5）：
//! - video 业务表 → `video.*` schema
//! - Phase 2A scaffold 阶段 **跳过 CREATE TABLE**（业务 entities 尚未 cp），仅创建空 schema
//! - Phase 2C 一次性 `ALTER TABLE public.X SET SCHEMA video;` 把 public 上 video 私有表迁过来
//! - public 共享表（media_playback_states / vfs / jobs / users 等）**不迁**，video 通过 bus call OS

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

pub mod entities;
pub mod models;
pub mod repos;

pub const SCHEMA: &str = "video";

pub async fn init_pool() -> anyhow::Result<DatabaseConnection> {
    let base_url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;

    let sep = if base_url.contains('?') { '&' } else { '?' };
    let url = format!("{base_url}{sep}application_name=tokimo-app-video");

    let mut opts = ConnectOptions::new(url);
    opts.max_connections(16).min_connections(2).sqlx_logging(false);

    Ok(Database::connect(opts).await?)
}

pub async fn init_schema(db: &DatabaseConnection) -> anyhow::Result<()> {
    let ddl = [format!(r#"CREATE SCHEMA IF NOT EXISTS "{SCHEMA}""#)];
    for sql in ddl {
        db.execute_raw(Statement::from_string(DatabaseBackend::Postgres, sql))
            .await?;
    }
    Ok(())
}
