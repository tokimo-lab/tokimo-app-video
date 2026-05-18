mod types;
pub use types::{StorageObject, StorageProvider, UploadOptions};

use std::path::Path;
use std::sync::Arc;

pub fn create_storage_from_env(_data_local_path: &Path) -> Arc<dyn StorageProvider> {
    Arc::new(NoopStorageProvider)
}

pub struct NoopStorageProvider;

#[async_trait::async_trait]
impl StorageProvider for NoopStorageProvider {
    async fn upload(&self, _key: &str, _body: bytes::Bytes, _options: Option<UploadOptions>) -> Result<(), String> {
        Ok(())
    }
    async fn download(&self, _key: &str) -> Result<bytes::Bytes, String> {
        Ok(bytes::Bytes::new())
    }
    async fn delete(&self, _key: &str) -> Result<(), String> {
        Ok(())
    }
    async fn exists(&self, _key: &str) -> Result<bool, String> {
        Ok(false)
    }
    async fn head(&self, _key: &str) -> Result<Option<StorageObject>, String> {
        Ok(None)
    }
    async fn list(&self, _prefix: Option<&str>) -> Result<Vec<StorageObject>, String> {
        Ok(vec![])
    }
}
