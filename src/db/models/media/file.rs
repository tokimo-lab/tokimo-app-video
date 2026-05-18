/// Resolved stream target for a media file — the minimal info needed to locate
/// and stream the physical file, plus metadata for `DirectPlay` progress tracking.
#[derive(Debug)]
pub struct MediaFileStreamTarget {
    pub path: String,
    pub source_id: Option<String>,
    pub source_type: Option<String>,
    pub source_config: Option<serde_json::Value>,
    /// For `DirectPlay` progress tracking (byte-offset → time estimation).
    pub video_item_id: Option<String>,
    pub episode_id: Option<String>,
    pub duration: Option<f64>,
    pub size: Option<i64>,
}
