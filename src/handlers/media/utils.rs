use uuid::Uuid;
use crate::AppState;
use crate::error::AppError;

/// Resolve the local filesystem path for a source-relative file path.
pub async fn resolve_local_path(
    _state: &AppState,
    _source_id: Uuid,
    _rel_path: &str,
) -> Result<String, AppError> {
    Err(AppError::Internal("resolve_local_path not implemented".into()))
}

/// Returns the local filesystem root path for a VFS driver, if applicable.
pub fn local_driver_root(fs: &crate::db::entities::vfs::Model) -> Option<String> {
    let json = fs.config.clone()?;
    let config: serde_json::Value = serde_json::from_value(json).ok()?;
    config.get("root")?.as_str().map(|s| s.to_string())
}
