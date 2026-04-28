//! Windows stub for [`iso_reader`] — Blu-ray ISO reading is not yet supported
//! on Windows because libudfread is a Unix-only C dependency.
//!
//! Mirrors the public surface of `iso_reader.rs` so `playback.rs` compiles
//! unchanged. All parsing functions return `Err`.

use tokimo_vfs::ReadAt;

#[derive(Debug, Clone)]
pub struct IsoExtent {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone)]
pub struct M2tsFile {
    pub filename: String,
    pub size: u64,
    pub extents: Vec<IsoExtent>,
}

pub fn find_m2ts_files(_read_at: ReadAt, _iso_size: u64) -> Result<Vec<M2tsFile>, String> {
    Err("Blu-ray ISO reading not supported on Windows yet".into())
}

pub fn select_main_m2ts(files: &[M2tsFile]) -> Option<&M2tsFile> {
    files.iter().max_by_key(|f| f.size)
}
