use serde::Serialize;
use tokimo_package_subtitle::types::EmbeddedSubtitleRecord;

/// The subtitle rows fetched for a media file, together with ffprobe data
/// needed to resolve embedded subtitle track indices.
#[derive(Debug)]
pub struct FileSubtitleRow {
    pub id: String,
    pub language: String,
    pub title: Option<String>,
    pub format: String,
    pub is_default: bool,
    pub is_forced: bool,
    pub source_id: Option<String>,
    pub ffprobe_raw: Option<serde_json::Value>,
}

impl FileSubtitleRow {
    pub fn to_embedded_record(&self) -> EmbeddedSubtitleRecord {
        EmbeddedSubtitleRecord {
            id: self.id.clone(),
            language: self.language.clone(),
            title: self.title.clone(),
            format: self.format.clone(),
            is_default: self.is_default,
            is_forced: self.is_forced,
            source_id: self.source_id.clone(),
        }
    }
}

/// A full subtitle record returned to the API client (matches `SubtitleOutputExtended` on the frontend).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleRecord {
    pub id: String,
    pub language: String,
    pub title: Option<String>,
    pub source_type: String,
    pub format: String,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
    pub stream_index: Option<i32>,
    pub storage_url: Option<String>,
    pub source: Option<String>,
    pub created_at: String,
}
