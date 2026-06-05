/// Resolve the local filesystem path for a source-relative file path.
pub fn resolve_local_path(rel_path: &str, config: Option<&serde_json::Value>) -> String {
    let driver_root = config
        .and_then(|c| c.get("root_folder_path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    
    if rel_path.starts_with('/') {
        format!("{}{}", driver_root.trim_end_matches('/'), rel_path)
    } else {
        format!("{}/{}", driver_root.trim_end_matches('/'), rel_path)
    }
}

/// Returns the local filesystem root path for a VFS driver, if applicable.
pub fn local_driver_root(fs: &crate::db::entities::vfs::Model) -> Option<String> {
    let json = fs.config.clone()?;
    let config: serde_json::Value = serde_json::from_value(json).ok()?;
    config.get("root")?.as_str().map(|s| s.to_string())
}
