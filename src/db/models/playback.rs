use serde::Serialize;

/// DTO for resume position query response.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct ResumePositionDto {
    pub position: i32,
    pub duration: Option<i32>,
    pub is_watched: bool,
    pub play_count: i32,
    pub last_watch_at: Option<String>,
}

/// DTO for watch history items.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WatchHistoryItemDto {
    pub id: String,
    pub file_id: Option<String>,
    pub user_name: Option<String>,
    pub client_name: Option<String>,
    pub user_agent: Option<String>,
    pub started_at: String,
    pub stopped_at: Option<String>,
    pub position: i32,
    pub duration: Option<i32>,
    pub completed: bool,
    /// Only present when querying by tv_show_id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_number: Option<i32>,
}

/// DTO for stream URL response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamUrlDto {
    pub url: String,
    /// The watch history record ID created/reused for this playback session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch_history_id: Option<String>,
}

/// Audio stream metadata parsed from the raw ffprobe JSON `audio_streams` column.
#[derive(Debug, Clone)]
pub struct AudioStreamInfo {
    pub index: i64,
    pub codec: String,
    pub channels: Option<i64>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i32>,
    pub profile: Option<String>,
    pub is_default: Option<bool>,
}

impl AudioStreamInfo {
    /// Parse from raw ffprobe JSON stored in the `audio_streams` column.
    /// Fields: `codec_name`, channels, tags.language, tags.title, `bit_rate` (string),
    /// `sample_rate` (string), disposition.default (int 0/1).
    pub fn from_json_array(val: Option<&serde_json::Value>) -> Vec<Self> {
        let Some(arr) = val.and_then(|v| v.as_array()) else {
            return vec![];
        };
        arr.iter()
            .enumerate()
            .filter_map(|(i, obj)| {
                let o = obj.as_object()?;
                let tags = o.get("tags").and_then(|v| v.as_object());
                let disposition = o.get("disposition").and_then(|v| v.as_object());
                Some(Self {
                    index: o.get("index").and_then(sea_orm::JsonValue::as_i64).unwrap_or(i as i64),
                    codec: o
                        .get("codec_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    channels: o.get("channels").and_then(sea_orm::JsonValue::as_i64),
                    language: tags
                        .and_then(|t| t.get("language"))
                        .and_then(|v| v.as_str())
                        .map(std::string::ToString::to_string),
                    title: tags
                        .and_then(|t| t.get("title"))
                        .and_then(|v| v.as_str())
                        .map(std::string::ToString::to_string),
                    bitrate: o
                        .get("bit_rate")
                        .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64())),
                    sample_rate: o
                        .get("sample_rate")
                        .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64())),
                    bit_depth: o.get("bits_per_raw_sample").and_then(|v| {
                        v.as_str()
                            .and_then(|s| s.parse::<i32>().ok())
                            .or_else(|| v.as_i64().map(|n| n as i32))
                    }),
                    profile: o
                        .get("profile")
                        .and_then(|v| v.as_str())
                        .map(std::string::ToString::to_string),
                    is_default: disposition
                        .and_then(|d| d.get("default"))
                        .and_then(sea_orm::JsonValue::as_i64)
                        .map(|v| v == 1),
                })
            })
            .collect()
    }
}
