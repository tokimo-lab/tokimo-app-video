pub mod datetime;
pub mod entities;
pub mod models;
pub mod pagination;
pub mod repos;

pub use datetime::{ApiDateTimeExt, OptionalApiDateTimeExt};

use sea_orm::{DatabaseConnection, ConnectOptions};

pub async fn init_pool() -> anyhow::Result<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let mut opt = ConnectOptions::new(url);
    opt.max_connections(20).min_connections(2);
    Ok(sea_orm::Database::connect(opt).await?)
}

pub async fn init_schema(_db: &DatabaseConnection) -> anyhow::Result<()> {
    Ok(())
}
