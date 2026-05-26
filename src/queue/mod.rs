pub mod priority;
pub use priority::JobPriority;
pub mod cancellation;
pub mod handlers;
pub mod online_media_download;
pub mod tmdb_person_scrape;
pub mod tv_scrape;
pub mod video_item_scrape;

use serde::Serialize;

use crate::db::models::job::JobOutput;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AppEvent {
    #[serde(rename = "job_update")]
    JobUpdate { job: Box<JobOutput> },
    #[serde(rename = "download_progress")]
    DownloadProgress { records: Vec<serde_json::Value> },
}
