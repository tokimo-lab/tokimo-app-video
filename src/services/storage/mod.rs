mod opendal_provider;
mod types;
pub use types::{StorageObject, StorageProvider, UploadOptions};

use std::path::Path;
use std::sync::Arc;
use tracing::info;

use opendal_provider::OpendalStorageProvider;

/// 从环境变量读取存储配置，创建对应的 `StorageProvider`。
///
/// 使用 `{data_local_path}/storage` 作为本地文件系统后端。
pub fn create_storage_from_env(data_local_path: &Path) -> Arc<dyn StorageProvider> {
    let base_path = data_local_path.join("storage");

    info!(
        "Storage: using local filesystem via OpenDAL (path={})",
        base_path.display()
    );

    Arc::new(OpendalStorageProvider::new(&base_path).expect("Storage provider initialization failed"))
}
