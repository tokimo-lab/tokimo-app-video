mod path;
pub mod storage_driver;
mod support;

use sea_orm::DatabaseConnection;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokimo_vfs::{Driver, Vfs};
use tokio::{
    sync::RwLock,
    time::{Duration, timeout},
};
use tracing::{info, warn};

use crate::db::models::media::vfs::{VfsRecord, VfsStatus};
use crate::db::repos::media::vfs_repo::VfsRepo;

pub use path::normalize_source_path;
use support::{build_vfs, build_vfs_driver, is_supported_source_type, vfs_fingerprint};

const DROP_DRIVER_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone)]
struct ManagedSource {
    source_type: String,
    fingerprint: String,
    driver: Arc<dyn Driver>,
    vfs: Arc<Vfs>,
}

pub type ManagedSourceStatus = VfsStatus;

pub struct SourceRegistry {
    db: DatabaseConnection,
    sources: RwLock<HashMap<String, ManagedSource>>,
}

impl SourceRegistry {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            sources: RwLock::new(HashMap::new()),
        }
    }

    pub async fn sync_all(&self) -> Result<Vec<ManagedSourceStatus>, String> {
        let records = VfsRepo::fetch_all(&self.db).await.map_err(|e| e.to_string())?;
        let desired: HashMap<String, VfsRecord> = records
            .into_iter()
            .filter(|record| is_supported_source_type(&record.vfs_type))
            .map(|record| (record.id.clone(), record))
            .collect();

        let current_ids: Vec<String> = self.sources.read().await.keys().cloned().collect();
        let desired_ids: HashSet<String> = desired.keys().cloned().collect();

        for source_id in current_ids {
            if !desired_ids.contains(&source_id) {
                let _ = self.disconnect_source(&source_id).await?;
            }
        }

        for record in desired.into_values() {
            if let Err(e) = self.sync_source_record(record).await {
                warn!("source sync failed: {e}");
            }
        }

        Ok(self.status_all().await)
    }

    pub async fn sync_source(&self, source_id: &str) -> Result<Option<ManagedSourceStatus>, String> {
        let record = VfsRepo::fetch_by_id(&self.db, source_id)
            .await
            .map_err(|e| e.to_string())?;
        let Some(record) = record else {
            let _ = self.disconnect_source(source_id).await?;
            return Ok(None);
        };

        if !is_supported_source_type(&record.vfs_type) {
            let _ = self.disconnect_source(source_id).await?;
            return Ok(None);
        }

        self.sync_source_record(record).await?;
        self.status_for(source_id).await.map(Some)
    }

    pub async fn disconnect_source(&self, source_id: &str) -> Result<bool, String> {
        let removed = { self.sources.write().await.remove(source_id) };
        if let Some(source) = removed {
            info!("disconnecting source {}", source_id);
            if !self.is_fingerprint_in_use(&source.fingerprint, Some(source_id)).await {
                match timeout(DROP_DRIVER_TIMEOUT, source.driver.drop_driver()).await {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        warn!("drop driver for {} failed: {}", source_id, err);
                    }
                    Err(_) => {
                        warn!(
                            "drop driver for {} timed out after {}s; source removed anyway",
                            source_id,
                            DROP_DRIVER_TIMEOUT.as_secs()
                        );
                    }
                }
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub async fn status_all(&self) -> Vec<ManagedSourceStatus> {
        let snapshot: Vec<(String, ManagedSource)> = self
            .sources
            .read()
            .await
            .iter()
            .map(|(source_id, managed)| (source_id.clone(), managed.clone()))
            .collect();

        let mut statuses = Vec::with_capacity(snapshot.len());
        for (source_id, managed) in snapshot {
            statuses.push(to_managed_status(&source_id, &managed).await);
        }
        statuses.sort_by(|a, b| a.id.cmp(&b.id));
        statuses
    }

    pub async fn status_for(&self, source_id: &str) -> Result<ManagedSourceStatus, String> {
        let managed = self
            .sources
            .read()
            .await
            .get(source_id)
            .cloned()
            .ok_or_else(|| format!("file system {source_id} is not connected"))?;
        Ok(to_managed_status(source_id, &managed).await)
    }

    pub async fn ensure_driver(&self, source_id: &str) -> Result<Arc<dyn Driver>, String> {
        if let Some(driver) = self
            .sources
            .read()
            .await
            .get(source_id)
            .map(|managed| Arc::clone(&managed.driver))
        {
            return Ok(driver);
        }

        let _ = self.sync_source(source_id).await?;

        self.sources
            .read()
            .await
            .get(source_id)
            .map(|managed| Arc::clone(&managed.driver))
            .ok_or_else(|| format!("file system {source_id} is not available"))
    }

    pub async fn ensure_vfs(&self, source_id: &str) -> Result<Arc<Vfs>, String> {
        if let Some(vfs) = self
            .sources
            .read()
            .await
            .get(source_id)
            .map(|managed| Arc::clone(&managed.vfs))
        {
            return Ok(vfs);
        }

        let _ = self.sync_source(source_id).await?;

        self.sources
            .read()
            .await
            .get(source_id)
            .map(|managed| Arc::clone(&managed.vfs))
            .ok_or_else(|| format!("file system {source_id} is not available"))
    }

    async fn sync_source_record(&self, record: VfsRecord) -> Result<(), String> {
        let fingerprint = vfs_fingerprint(&record);
        if let Some(existing) = self.sources.read().await.get(&record.id).cloned()
            && existing.fingerprint == fingerprint
        {
            return Ok(());
        }

        let (driver, is_new) =
            if let Some(shared_driver) = self.find_driver_by_fingerprint(&fingerprint, Some(&record.id)).await {
                info!("source {} reusing existing connection", record.id);
                (shared_driver, false)
            } else {
                // Build the persister closure before init() so the driver can call
                // it immediately when credentials rotate (e.g. Baidu refresh_token
                // on first token exchange, Quark cookie on every response).
                // This is the equivalent of OpenList's op.MustSaveDriverStorage.
                let persister = if record.vfs_type == "baidu_netdisk" || record.vfs_type == "quark" {
                    let db = self.db.clone();
                    let record_id = record.id.clone();
                    let p: tokimo_vfs::ConfigPersister = Arc::new(move |patch| {
                        let db = db.clone();
                        let id = record_id.clone();
                        tokio::spawn(async move {
                            if let Err(err) = VfsRepo::patch_config(&db, &id, patch).await {
                                warn!("failed to persist config for {}: {}", id, err);
                            }
                        });
                    });
                    Some(p)
                } else {
                    None
                };
                (build_vfs_driver(&record, persister).await?, true)
            };

        // Persist one-shot credentials (e.g. QR-code login cookie) that were
        // resolved during init() but not covered by the persister callback.
        if is_new
            && let Some(patch) = driver.resolved_config_patch()
            && let Err(err) = VfsRepo::patch_config(&self.db, &record.id, patch).await
        {
            warn!("failed to persist config patch for {}: {}", record.id, err);
        }

        let managed = ManagedSource {
            source_type: record.vfs_type.clone(),
            fingerprint: fingerprint.clone(),
            vfs: build_vfs(Arc::clone(&driver)).await,
            driver,
        };

        let previous = { self.sources.write().await.insert(record.id.clone(), managed.clone()) };

        if let Some(old) = previous
            && old.fingerprint != fingerprint
            && !self.is_fingerprint_in_use(&old.fingerprint, Some(&record.id)).await
        {
            match timeout(DROP_DRIVER_TIMEOUT, old.driver.drop_driver()).await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    warn!("drop old driver for {} failed: {}", record.id, err);
                }
                Err(_) => {
                    warn!(
                        "drop old driver for {} timed out after {}s",
                        record.id,
                        DROP_DRIVER_TIMEOUT.as_secs()
                    );
                }
            }
        }

        info!("source {} synchronized", record.id);

        Ok(())
    }

    async fn find_driver_by_fingerprint(
        &self,
        fingerprint: &str,
        exclude_source_id: Option<&str>,
    ) -> Option<Arc<dyn Driver>> {
        self.sources
            .read()
            .await
            .iter()
            .find(|(source_id, managed)| {
                exclude_source_id != Some(source_id.as_str()) && managed.fingerprint == fingerprint
            })
            .map(|(_, managed)| Arc::clone(&managed.driver))
    }

    async fn is_fingerprint_in_use(&self, fingerprint: &str, exclude_source_id: Option<&str>) -> bool {
        self.sources.read().await.iter().any(|(source_id, managed)| {
            exclude_source_id != Some(source_id.as_str()) && managed.fingerprint == fingerprint
        })
    }
}

async fn to_managed_status(source_id: &str, managed: &ManagedSource) -> ManagedSourceStatus {
    let status = managed.driver.status().await;
    VfsStatus {
        id: source_id.to_string(),
        r#type: managed.source_type.clone(),
        driver: status.driver,
        state: format!("{:?}", status.state).to_lowercase(),
        error: status.error,
    }
}
