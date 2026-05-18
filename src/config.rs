use crate::db::repos::system_config_repo::SystemConfigSection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TmdbSettings {
    pub api_key: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScrapingSettings {
    pub generate_nfo: bool,
    pub download_images: bool,
    pub use_chinese_info: bool,
    pub movie_image_sources: Option<Vec<String>>,
    pub tv_image_sources: Option<Vec<String>>,
    pub image_language_priority: Option<Vec<String>>,
    pub use_hd_poster: bool,
    pub use_hd_backdrop: bool,
}

impl Default for ScrapingSettings {
    fn default() -> Self {
        Self {
            generate_nfo: true,
            download_images: true,
            use_chinese_info: false,
            movie_image_sources: None,
            tv_image_sources: None,
            image_language_priority: None,
            use_hd_poster: true,
            use_hd_backdrop: false,
        }
    }
}

use chrono::{DateTime, FixedOffset};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    pub allow_registration: bool,
    pub internal_stream_access_token: Option<String>,
    pub internal_stream_access_token_expires_at: Option<DateTime<FixedOffset>>,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            allow_registration: false,
            internal_stream_access_token: None,
            internal_stream_access_token_expires_at: None,
        }
    }
}

impl SystemConfigSection for TmdbSettings {
    const SCOPE: &'static str = "metadata";
    const SCOPE_ID: &'static str = "tmdb";
    fn default_value() -> Self {
        Self {
            api_key: None,
            language: None,
        }
    }
}

impl SystemConfigSection for ScrapingSettings {
    const SCOPE: &'static str = "metadata";
    const SCOPE_ID: &'static str = "scraping";
    fn default_value() -> Self {
        Self::default()
    }
}

impl SystemConfigSection for SystemSettings {
    const SCOPE: &'static str = "system";
    const SCOPE_ID: &'static str = "main";
    fn default_value() -> Self {
        Self::default()
    }
}
