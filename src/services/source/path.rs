use std::path::Path;

pub fn normalize_source_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    let normalized = if trimmed.is_empty() { "/" } else { trimmed };
    let path = Path::new(normalized);
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::RootDir | std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => {
                parts.push(part.to_string_lossy().to_string());
            }
            std::path::Component::ParentDir => return Err("path must not contain parent traversal ('..')".into()),
            std::path::Component::Prefix(_) => return Err("path contains an unsupported path prefix".into()),
        }
    }
    if parts.is_empty() {
        return Ok("/".into());
    }
    Ok(format!("/{}", parts.join("/")))
}
