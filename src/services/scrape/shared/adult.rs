//! Adult (R18) metadata lookup: video-ID extraction + chained JavBus → JavDB → TPDB → StashDB search.
//!
//! Ported from the legacy TS server (`video-id-extractor.ts`, `adult-metadata.service.ts`).

use regex::Regex;
use sea_orm::DatabaseConnection;
use serde_json::json;
use std::sync::{Arc, LazyLock};
use tokimo_media_scraper::metadata_providers::javbus::{JavBusClient, JavBusConfig};
use tokimo_media_scraper::metadata_providers::javdb::{JavDBClient, JavDBConfig};
use tokimo_media_scraper::metadata_providers::stashdb::{StashDBClient, StashDBConfig};
use tokimo_media_scraper::metadata_providers::tpdb::{TpdbClient, TpdbConfig};
use tokimo_media_scraper::types::AdultMetadata;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::AppState;
use crate::config::AdultMetadataSettings;
use crate::db::repos::system_config_repo::SystemConfigRepo;

// ── Video ID extraction ──────────────────────────────────────────────────────

/// Patterns ordered by priority.
static VIDEO_ID_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // FC2-PPV-1234567, FC2PPV-1234567, FC2PPV1234567
        r"(?i)\bFC2-?PPV-?(\d{5,7})\b",
        // Caribbeancom: 123456-789
        r"\b(\d{6}-\d{3})\b",
        // HEYZO-1234
        r"(?i)\b(HEYZO-\d{4})\b",
        // Tokyo-Hot: n1234, k1234
        r"\b([nk]\d{4})\b",
        // 1Pondo: 123456_789
        r"\b(\d{6}_\d{3})\b",
        // Generic JAV: SSIS-001, ABW-123, IPX789
        r"(?i)\b([A-Z]{2,6})-?(\d{3,5})\b",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("valid video id regex"))
    .collect()
});

/// Codec / resolution / container tokens that look like `PREFIX-123`.
const FALSE_POSITIVE_PREFIXES: &[&str] = &[
    "AVC", "HEVC", "AAC", "DTS", "FLAC", "MP4", "MKV", "AVI", "WMV", "FLV", "MOV", "RMVB", "ISO", "VOB", "HDR", "SDR",
    "BLU", "WEB", "DVD", "CAM", "FHD", "UHD",
];

fn normalize_video_id(caps: &regex::Captures<'_>) -> Option<String> {
    let full = caps.get(0)?.as_str();
    let upper = full.to_uppercase();

    if upper.starts_with("FC2") {
        return Some(format!("FC2-PPV-{}", caps.get(1)?.as_str()));
    }
    if full.len() == 10 && full.chars().filter(char::is_ascii_digit).count() == 9 {
        // Caribbeancom / 1Pondo keep their original form.
        return Some(full.to_string());
    }
    if upper.starts_with("HEYZO") {
        return Some(upper);
    }
    if let (Some(prefix), Some(num)) = (caps.get(1), caps.get(2)) {
        let prefix = prefix.as_str().to_uppercase();
        if FALSE_POSITIVE_PREFIXES.contains(&prefix.as_str()) {
            return None;
        }
        return Some(format!("{prefix}-{}", num.as_str()));
    }
    // Tokyo-Hot
    Some(full.to_lowercase())
}

/// Extract a normalized video ID (番号) from a file name or title.
pub fn extract_video_id(text: &str) -> Option<String> {
    VIDEO_ID_PATTERNS
        .iter()
        .filter_map(|re| re.captures(text))
        .find_map(|caps| normalize_video_id(&caps))
}

// ── Clients ──────────────────────────────────────────────────────────────────

pub struct AdultClients {
    javbus: Option<JavBusClient>,
    javdb: JavDBClient,
    tpdb: Option<TpdbClient>,
    stashdb: StashDBClient,
}

type ClientCache = Option<(AdultMetadataSettings, Arc<AdultClients>)>;

/// Clients are cached (they hold request caches) and rebuilt only when settings change.
static CLIENTS: LazyLock<Mutex<ClientCache>> = LazyLock::new(|| Mutex::new(None));

fn non_empty(s: Option<&String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty()).cloned()
}

fn build_clients(state: &AppState, s: &AdultMetadataSettings) -> AdultClients {
    let flaresolverr_url = non_empty(s.flaresolverr_url.as_ref());
    AdultClients {
        // JavBus needs a browser cookie to get past the age gate.
        javbus: non_empty(s.javbus_cookie.as_ref()).map(|cookie| {
            JavBusClient::new(JavBusConfig {
                base_url: non_empty(s.javbus_base_url.as_ref()),
                language: None,
                cookie: Some(cookie),
                flaresolverr_url: flaresolverr_url.clone(),
            })
        }),
        javdb: JavDBClient::new(JavDBConfig {
            base_url: non_empty(s.javdb_base_url.as_ref()),
            language: None,
            cookie: non_empty(s.javdb_cookie.as_ref()),
            flaresolverr_url,
        }),
        tpdb: non_empty(s.tpdb_api_key.as_ref()).map(|api_key| {
            TpdbClient::new(TpdbConfig {
                api_key,
                base_url: None,
                cache_ttl: None,
                http_client: state.http_client.clone(),
            })
        }),
        stashdb: StashDBClient::new(StashDBConfig {
            api_key: non_empty(s.stashdb_api_key.as_ref()),
            base_url: None,
            cache_ttl: None,
            http_client: state.http_client.clone(),
        }),
    }
}

/// Returns the adult clients, or `None` when adult metadata is disabled in settings.
pub async fn load_clients(db: &DatabaseConnection, state: &AppState) -> Option<Arc<AdultClients>> {
    let settings = match SystemConfigRepo::get::<AdultMetadataSettings>(db).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[adult] failed to load adult metadata settings: {e}");
            return None;
        }
    };
    if !settings.enabled {
        debug!("[adult] adult metadata disabled in settings");
        return None;
    }

    let mut guard = CLIENTS.lock().await;
    if let Some((cached, clients)) = guard.as_ref()
        && *cached == settings
    {
        return Some(Arc::clone(clients));
    }
    let clients = Arc::new(build_clients(state, &settings));
    *guard = Some((settings, Arc::clone(&clients)));
    Some(clients)
}

impl AdultClients {
    /// Chained lookup; the first source returning a result with a poster wins.
    pub async fn search(&self, video_id: &str) -> Option<AdultMetadata> {
        macro_rules! try_source {
            ($name:literal, $fut:expr) => {
                match $fut.await {
                    Ok(Some(meta)) if meta.poster_url.is_some() => return Some(meta),
                    Ok(_) => debug!("[adult] {} has no result for {video_id}", $name),
                    Err(e) => warn!("[adult] {} lookup failed for {video_id}: {e}", $name),
                }
            };
        }

        if let Some(c) = &self.javbus {
            try_source!("javbus", c.search_by_video_id(video_id));
        }
        try_source!("javdb", self.javdb.search_by_video_id(video_id));
        if let Some(c) = &self.tpdb {
            try_source!("tpdb", c.search_by_video_id(video_id));
        }
        try_source!("stashdb", self.stashdb.search_by_video_id(video_id));
        None
    }

    /// Fallback poster from the JavBus search page when the derived poster URL is dead.
    pub async fn fetch_thumb_from_search(&self, video_id: &str) -> Option<String> {
        let c = self.javbus.as_ref()?;
        c.fetch_thumb_url_from_search(video_id).await.ok().flatten()
    }
}

/// Serialize for the web UI (camelCase, images routed through the signed image proxy).
pub fn to_ui_json(meta: &AdultMetadata, proxy_key: &str) -> serde_json::Value {
    let proxy = |u: &Option<String>| {
        u.as_deref()
            .map(|u| crate::handlers::image_proxy::to_proxy_url_force(proxy_key, u))
    };
    json!({
        "videoId": meta.video_id,
        "title": meta.title,
        "posterUrl": proxy(&meta.poster_url),
        "coverUrl": proxy(&meta.cover_url),
        "sourceUrl": meta.source_url,
        "actors": meta.actors,
        "genres": meta.genres,
        "releaseDate": meta.release_date,
        "studio": meta.studio,
        "duration": meta.duration,
        "rating": meta.rating,
        "source": meta.source,
    })
}

#[cfg(test)]
mod tests {
    use super::extract_video_id;

    #[test]
    fn extracts_common_video_ids() {
        let cases = [
            ("SSIS-001.mp4", Some("SSIS-001")),
            ("[HD] ipx789 1080p.mkv", Some("IPX-789")),
            ("abw-123-C.mp4", Some("ABW-123")),
            ("FC2PPV1234567.mp4", Some("FC2-PPV-1234567")),
            ("fc2-ppv-123456 part1.mkv", Some("FC2-PPV-123456")),
            ("Carib 123456-789.mp4", Some("123456-789")),
            ("123456_789-1pon.mp4", Some("123456_789")),
            ("heyzo-1234.mp4", Some("HEYZO-1234")),
            ("n1234.avi", Some("n1234")),
            ("MKV-2160 random.mkv", None),
            ("Some Movie (2020).mkv", None),
        ];
        for (input, expected) in cases {
            assert_eq!(extract_video_id(input).as_deref(), expected, "input: {input}");
        }
    }
}
