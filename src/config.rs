use crate::db::repos::scrape_settings_repo::ScrapeSettingsSection;
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
#[derive(Default)]
pub struct SystemSettings {
    pub allow_registration: bool,
    pub internal_stream_access_token: Option<String>,
    pub internal_stream_access_token_expires_at: Option<DateTime<FixedOffset>>,
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

impl ScrapeSettingsSection for ScrapingSettings {
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

/// Adult metadata scraping settings (`JavBus`, `JavDB`, TPDB, `StashDB`).
/// Mirrors the host server's `metadata/adult` system config section.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AdultMetadataSettings {
    pub enabled: bool,
    pub javbus_base_url: Option<String>,
    pub javbus_cookie: Option<String>,
    pub javdb_base_url: Option<String>,
    pub javdb_cookie: Option<String>,
    pub tpdb_api_key: Option<String>,
    pub stashdb_api_key: Option<String>,
    pub daily_limit: Option<i32>,
    pub flaresolverr_url: Option<String>,
}

impl SystemConfigSection for AdultMetadataSettings {
    const SCOPE: &'static str = "metadata";
    const SCOPE_ID: &'static str = "adult";
    fn default_value() -> Self {
        Self::default()
    }
}
