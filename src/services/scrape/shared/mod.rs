//! Shared utilities for all media scraping domains.

pub mod artwork;
pub mod constants;
pub mod episode_screenshot;
pub mod lib_type;
pub mod media_file;
pub mod parse;
pub mod subtitle;
pub mod tmdb;

use std::sync::Arc;

/// Shared directory context for VFS operations.
pub struct DirContext {
    pub vfs: Arc<next_fs::Vfs>,
    pub dir_path: String,
    pub dir_entries: Vec<String>,
    pub stem: String,
}
