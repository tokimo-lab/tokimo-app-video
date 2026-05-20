pub mod datetime;
pub mod entities;
pub mod models;
pub mod pagination;
pub mod repos;

pub use datetime::{ApiDateTimeExt, OptionalApiDateTimeExt};

use sea_orm::{DatabaseConnection, ConnectOptions};

pub async fn init_pool() -> anyhow::Result<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    // Pin search_path to `video, public` so:
    //   1. Raw SQL (`FROM videos`, `FROM tv_shows`, …) resolves to the `video` schema.
    //   2. Residual cross-schema reads (`jobs`, etc.) still hit `public` as fallback.
    // Each pooled connection picks this up via the libpq `options` URL parameter.
    let separator = if url.contains('?') { '&' } else { '?' };
    let url_with_search_path = format!("{url}{separator}options=-csearch_path%3Dvideo%2Cpublic");
    let mut opt = ConnectOptions::new(url_with_search_path);
    opt.max_connections(20).min_connections(2);
    Ok(sea_orm::Database::connect(opt).await?)
}

pub async fn init_schema(_db: &DatabaseConnection) -> anyhow::Result<()> {
    Ok(())
}
