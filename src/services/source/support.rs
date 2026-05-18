use std::sync::Arc;
use tokimo_vfs::{ConfigPersister, Driver, DriverRegistry, StorageManager, StorageMount, Vfs};

use crate::db::models::media::vfs::VfsRecord;

const SUPPORTED_SOURCE_TYPES: [&str; 12] = [
    "local",
    "smb",
    "nfs",
    "webdav",
    "ftp",
    "sftp",
    "s3",
    "115cloud",
    "aliyundrive",
    "baidu_netdisk",
    "quark",
    "189cloud",
];

/// Drivers that rotate credentials at runtime (Baidu refresh_token, Quark
/// cookie) must NOT include those fields in the fingerprint — the rotated
/// value is the same logical connection, not a new one.
const VOLATILE_CONFIG_KEYS: &[&str] = &["refresh_token", "cookie"];

pub fn vfs_fingerprint(record: &VfsRecord) -> String {
    let config_str = if let Some(obj) = record.config.as_object() {
        let stable: serde_json::Map<String, serde_json::Value> = obj
            .iter()
            .filter(|(k, _)| !VOLATILE_CONFIG_KEYS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        serde_json::to_string(&stable).unwrap_or_default()
    } else {
        serde_json::to_string(&record.config).unwrap_or_default()
    };
    format!("{}:{}", record.vfs_type, config_str)
}

pub fn is_supported_source_type(source_type: &str) -> bool {
    SUPPORTED_SOURCE_TYPES.contains(&source_type)
}

pub async fn build_vfs_driver(
    record: &VfsRecord,
    persister: Option<ConfigPersister>,
) -> Result<Arc<dyn Driver>, String> {
    let registry = DriverRegistry::new();
    let driver = registry
        .create(&record.vfs_type, &record.config)
        .map_err(|err| err.to_string())?;
    let driver: Arc<dyn Driver> = Arc::from(driver);
    if let Some(p) = persister {
        driver.set_config_persister(p);
    }
    driver.init().await.map_err(|err| err.to_string())?;
    Ok(driver)
}

pub async fn build_vfs(driver: Arc<dyn Driver>) -> Arc<Vfs> {
    let manager = StorageManager::new();
    manager.mount(StorageMount::new("/", driver)).await;
    Arc::new(Vfs::new(manager))
}
