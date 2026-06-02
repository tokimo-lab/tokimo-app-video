//! A `tokimo_vfs::Driver` implementation backed by `StorageProvider`.
//!
//! Bridges the application's `StorageProvider` trait (simple upload/download/list)
//! to the VFS `Driver` trait (file system semantics with list/stat/read/write).
//!
//! Optionally wraps a `WriteCallback` to notify business logic when files are
//! written or deleted via VFS.
//!
//! ## Attachment manifests
//!
//! Directories may contain a `.attachments.json` file that maps virtual
//! filenames to their real storage keys. This allows the VFS to present
//! attachments alongside markdown files without duplicating data. The
//! manifest file itself is hidden from directory listings.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tokimo_vfs_core::driver::traits::{DeleteFile, Driver, Meta, Mkdir, PutFile, Reader};
use tokimo_vfs_core::error::{Result, TokimoVfsError};
use tokimo_vfs_core::model::obj::FileInfo;
use tokimo_vfs_core::model::storage::{ConnectionState, StorageCapabilities, StorageStatus};
use tracing::{error, warn};

use crate::apps::docs::services::markdown_sync::ATTACHMENTS_MANIFEST;
use tokimo_package_storage::StorageProvider;

/// Callback interface for VFS write events.
///
/// Implement this to react to file changes made through VFS (e.g., sync
/// written markdown back to the database).
#[async_trait]
pub trait WriteCallback: Send + Sync + 'static {
    /// Called after a file is written or created via VFS.
    async fn on_file_written(&self, relative_path: &str, content: &[u8]) -> std::result::Result<(), String>;

    /// Called after a file is deleted via VFS.
    async fn on_file_deleted(&self, relative_path: &str) -> std::result::Result<(), String>;
}

/// A `Driver` that maps a prefix of a `StorageProvider` as a virtual filesystem.
///
/// - `prefix`: S3 key prefix (e.g. `docs-md/my-notes`), no trailing slash
/// - `writable`: whether VFS write ops are allowed
/// - `on_write`: optional callback triggered after writes (async, non-blocking)
pub struct StorageProviderDriver {
    storage: Arc<dyn StorageProvider>,
    prefix: String,
    writable: bool,
    on_write: Option<Arc<dyn WriteCallback>>,
}

impl StorageProviderDriver {
    pub fn new(
        storage: Arc<dyn StorageProvider>,
        prefix: String,
        writable: bool,
        on_write: Option<Arc<dyn WriteCallback>>,
    ) -> Self {
        let prefix = prefix.trim_matches('/').to_string();
        Self {
            storage,
            prefix,
            writable,
            on_write,
        }
    }

    /// Build full storage key from a VFS path.
    fn storage_key(&self, path: &Path) -> String {
        let relative = path.to_string_lossy().trim_start_matches('/').to_string();
        if relative.is_empty() {
            self.prefix.clone()
        } else if self.prefix.is_empty() {
            relative
        } else {
            format!("{}/{relative}", self.prefix)
        }
    }

    /// Build storage prefix for directory listing.
    fn dir_prefix(&self, path: &Path) -> String {
        let key = self.storage_key(path);
        if key.is_empty() {
            String::new()
        } else {
            format!("{key}/")
        }
    }

    /// Extract relative path from a storage key (strip prefix).
    #[allow(dead_code)]
    fn relative_path(&self, key: &str) -> String {
        let stripped = if self.prefix.is_empty() {
            key.to_string()
        } else {
            key.strip_prefix(&self.prefix)
                .unwrap_or(key)
                .trim_start_matches('/')
                .to_string()
        };
        format!("/{stripped}")
    }

    /// Spawn async write callback (fire-and-forget).
    fn notify_written(&self, relative_path: &str, content: &[u8]) {
        if let Some(ref cb) = self.on_write {
            let cb = cb.clone();
            let path = relative_path.to_string();
            let data = content.to_vec();
            tokio::spawn(async move {
                if let Err(e) = cb.on_file_written(&path, &data).await {
                    error!("WriteCallback.on_file_written failed for {path}: {e}");
                }
            });
        }
    }

    fn notify_deleted(&self, relative_path: &str) {
        if let Some(ref cb) = self.on_write {
            let cb = cb.clone();
            let path = relative_path.to_string();
            tokio::spawn(async move {
                if let Err(e) = cb.on_file_deleted(&path).await {
                    error!("WriteCallback.on_file_deleted failed for {path}: {e}");
                }
            });
        }
    }

    /// Load the `.attachments.json` manifest for a directory, if it exists.
    ///
    /// Returns a map of `{ virtual_filename → source_storage_key }`.
    /// The manifest file itself is hidden from directory listings.
    async fn load_attachment_manifest(&self, dir_path: &Path) -> HashMap<String, String> {
        let manifest_key = if dir_path.to_string_lossy() == "/" || dir_path.to_string_lossy().is_empty() {
            format!("{}/{ATTACHMENTS_MANIFEST}", self.prefix)
        } else {
            self.storage_key(&dir_path.join(ATTACHMENTS_MANIFEST))
        };

        let Ok(data) = self.storage.download(&manifest_key).await else {
            return HashMap::new();
        };

        serde_json::from_slice::<HashMap<String, String>>(&data).unwrap_or_else(|e| {
            warn!("Failed to parse {ATTACHMENTS_MANIFEST}: {e}");
            HashMap::new()
        })
    }

    /// Resolve a file path to a real storage key, checking attachment manifests.
    ///
    /// If the filename is listed in the parent directory's `.attachments.json`,
    /// returns the mapped source storage key. Otherwise returns `None`.
    async fn resolve_attachment_key(&self, path: &Path) -> Option<String> {
        let parent = path.parent()?;
        let filename = path.file_name()?.to_string_lossy().to_string();
        let manifest = self.load_attachment_manifest(parent).await;
        manifest.get(&filename).cloned()
    }
}

// ── Meta ────────────────────────────────────────────────────────────────────

#[async_trait]
impl Meta for StorageProviderDriver {
    fn driver_name(&self) -> &'static str {
        "storage_provider"
    }

    async fn init(&self) -> Result<()> {
        // Verify the prefix is accessible by listing it
        self.storage
            .list(Some(&self.dir_prefix(Path::new("/"))))
            .await
            .map_err(|e| TokimoVfsError::Other(format!("storage init failed: {e}")))?;
        Ok(())
    }

    async fn drop_driver(&self) -> Result<()> {
        Ok(())
    }

    async fn status(&self) -> StorageStatus {
        StorageStatus {
            driver: "storage_provider".into(),
            state: ConnectionState::Connected,
            error: None,
            capabilities: self.capabilities(),
        }
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities {
            list: true,
            read: true,
            mkdir: false, // storage provider doesn't have explicit mkdir
            delete_file: self.writable,
            delete_dir: false,
            rename: false,
            write: self.writable,
            symlink: false,
            range_read: false,
        }
    }
}

// ── Reader ──────────────────────────────────────────────────────────────────

#[async_trait]
impl Reader for StorageProviderDriver {
    async fn list(&self, path: &Path) -> Result<Vec<FileInfo>> {
        let prefix = self.dir_prefix(path);
        let objects = self
            .storage
            .list(Some(&prefix))
            .await
            .map_err(|e| TokimoVfsError::Other(format!("list failed: {e}")))?;

        let mut entries = Vec::new();
        let mut seen_dirs = std::collections::HashSet::new();
        let mut seen_files = std::collections::HashSet::new();

        for obj in objects {
            let rel = if prefix.is_empty() {
                obj.key.clone()
            } else {
                obj.key.strip_prefix(&prefix).unwrap_or(&obj.key).to_string()
            };

            if rel.is_empty() {
                continue;
            }

            // Hide the attachment manifest from directory listings
            if rel == ATTACHMENTS_MANIFEST {
                continue;
            }

            if let Some(slash_pos) = rel.find('/') {
                let dir_name = &rel[..slash_pos];
                if !dir_name.is_empty() && seen_dirs.insert(dir_name.to_string()) {
                    let display_path = format!("{}/{}", path.to_string_lossy().trim_end_matches('/'), dir_name);
                    entries.push(FileInfo {
                        name: dir_name.to_string(),
                        path: if display_path.starts_with('/') {
                            display_path
                        } else {
                            format!("/{display_path}")
                        },
                        size: 0,
                        is_dir: true,
                        modified: None,
                    });
                }
            } else {
                seen_files.insert(rel.clone());
                let display_path = format!("{}/{}", path.to_string_lossy().trim_end_matches('/'), rel);
                entries.push(FileInfo {
                    name: rel,
                    path: if display_path.starts_with('/') {
                        display_path
                    } else {
                        format!("/{display_path}")
                    },
                    size: obj.size,
                    is_dir: false,
                    modified: None,
                });
            }
        }

        // Inject virtual attachment entries from .attachments.json manifest
        let manifest = self.load_attachment_manifest(path).await;
        for (filename, source_key) in &manifest {
            if seen_files.contains(filename.as_str()) || seen_dirs.contains(filename.as_str()) {
                continue; // Real file takes precedence
            }
            // Get size from the source storage key
            let size = self.storage.head(source_key).await.ok().flatten().map_or(0, |o| o.size);
            let display_path = format!("{}/{}", path.to_string_lossy().trim_end_matches('/'), filename);
            entries.push(FileInfo {
                name: filename.clone(),
                path: if display_path.starts_with('/') {
                    display_path
                } else {
                    format!("/{display_path}")
                },
                size,
                is_dir: false,
                modified: None,
            });
        }

        Ok(entries)
    }

    async fn stat(&self, path: &Path) -> Result<FileInfo> {
        let path_str = path.to_string_lossy();
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();

        // Root is always a directory
        if path_str == "/" || path_str.is_empty() {
            return Ok(FileInfo {
                name: String::new(),
                path: "/".to_string(),
                size: 0,
                is_dir: true,
                modified: None,
            });
        }

        let display = format!("/{}", path.to_string_lossy().trim_start_matches('/'));

        // Try as directory first (check if any children exist under this prefix).
        // This must come before the file check because some backends (e.g. local FS
        // via OpenDAL) return `exists=true` for directories, which would incorrectly
        // classify them as files.
        let prefix = self.dir_prefix(path);
        let children = self
            .storage
            .list(Some(&prefix))
            .await
            .map_err(|e| TokimoVfsError::Other(format!("stat list failed: {e}")))?;

        if !children.is_empty() {
            return Ok(FileInfo {
                name,
                path: display,
                size: 0,
                is_dir: true,
                modified: None,
            });
        }

        // Try as file (use head to get actual size)
        let key = self.storage_key(path);
        if let Ok(Some(obj)) = self.storage.head(&key).await {
            return Ok(FileInfo {
                name,
                path: display,
                size: obj.size,
                is_dir: false,
                modified: None,
            });
        }

        // Try as virtual attachment (mapped via .attachments.json)
        if let Some(source_key) = self.resolve_attachment_key(path).await {
            let size = self
                .storage
                .head(&source_key)
                .await
                .ok()
                .flatten()
                .map_or(0, |o| o.size);
            return Ok(FileInfo {
                name,
                path: display,
                size,
                is_dir: false,
                modified: None,
            });
        }

        Err(TokimoVfsError::NotFound(format!("path not found: {}", path.display())))
    }

    async fn read_bytes(&self, path: &Path, offset: u64, _limit: Option<u64>) -> Result<Vec<u8>> {
        // Check if this file is a virtual attachment first
        let key = if let Some(source_key) = self.resolve_attachment_key(path).await {
            source_key
        } else {
            self.storage_key(path)
        };

        let data = self
            .storage
            .download(&key)
            .await
            .map_err(|e| TokimoVfsError::NotFound(format!("read failed: {e}")))?;

        if offset > 0 {
            let start = offset as usize;
            if start >= data.len() {
                return Ok(Vec::new());
            }
            Ok(data[start..].to_vec())
        } else {
            Ok(data.to_vec())
        }
    }
}

// ── Write capabilities ──────────────────────────────────────────────────────

#[async_trait]
impl PutFile for StorageProviderDriver {
    async fn put(&self, path: &Path, data: Vec<u8>) -> Result<()> {
        let key = self.storage_key(path);
        let relative = path.to_string_lossy().trim_start_matches('/').to_string();

        self.storage
            .upload(&key, bytes::Bytes::from(data.clone()), None)
            .await
            .map_err(|e| TokimoVfsError::Other(format!("write failed: {e}")))?;

        self.notify_written(&relative, &data);
        Ok(())
    }
}

#[async_trait]
impl DeleteFile for StorageProviderDriver {
    async fn delete_file(&self, path: &Path) -> Result<()> {
        let key = self.storage_key(path);
        let relative = path.to_string_lossy().trim_start_matches('/').to_string();

        self.storage
            .delete(&key)
            .await
            .map_err(|e| TokimoVfsError::Other(format!("delete failed: {e}")))?;

        self.notify_deleted(&relative);
        Ok(())
    }
}

#[async_trait]
impl Mkdir for StorageProviderDriver {
    async fn mkdir(&self, path: &Path) -> Result<()> {
        // Storage providers don't have explicit directories.
        // Create a zero-byte marker object to represent the directory.
        let key = format!("{}/", self.storage_key(path));
        self.storage
            .upload(&key, bytes::Bytes::new(), None)
            .await
            .map_err(|e| TokimoVfsError::Other(format!("mkdir failed: {e}")))?;
        Ok(())
    }
}

// ── Driver trait (wire capabilities) ────────────────────────────────────────

impl Driver for StorageProviderDriver {
    fn as_put(&self) -> Option<&dyn PutFile> {
        if self.writable { Some(self) } else { None }
    }

    fn as_delete_file(&self) -> Option<&dyn DeleteFile> {
        if self.writable { Some(self) } else { None }
    }

    fn as_mkdir(&self) -> Option<&dyn Mkdir> {
        if self.writable { Some(self) } else { None }
    }
}
