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
