use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use hmac::{Hmac, Mac, digest::KeyInit};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use sha2::Sha256;
use tracing::warn;
use url::Url;

use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

const PROXY_PREFIX: &str = "/api/apps/video/image-proxy";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

const ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

#[derive(Deserialize)]
pub struct ImageProxyParams {
    pub url: Option<String>,
    pub sig: Option<String>,
}

pub fn sign_proxy_url(key: &str, url: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes())
        .expect("HMAC accepts keys of any size");
    mac.update(url.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn verify_signature(key: &str, url: &str, sig: &str) -> bool {
    let Ok(sig_bytes) = hex::decode(sig) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(key.as_bytes())
        .expect("HMAC accepts keys of any size");
    mac.update(url.as_bytes());
    mac.verify_slice(&sig_bytes).is_ok()
}

pub fn to_proxy_url_force(key: &str, original_url: &str) -> String {
    if original_url.is_empty() || original_url.starts_with("data:") || is_proxy_url(original_url) {
        return original_url.to_string();
    }

    format!(
        "{PROXY_PREFIX}?url={}&sig={}",
        utf8_percent_encode(original_url, ENCODE_SET),
        sign_proxy_url(key, original_url)
    )
}

fn is_proxy_url(url: &str) -> bool {
    url.starts_with(PROXY_PREFIX)
        || Url::parse(url)
            .map(|parsed| parsed.path() == PROXY_PREFIX)
            .unwrap_or(false)
}

/// Unwraps a proxy URL to extract the original URL, if applicable.
pub fn unwrap_proxy_url(proxy_url: &str) -> Option<String> {
    let parsed = if proxy_url.starts_with(PROXY_PREFIX) {
        Url::parse(&format!("http://localhost{proxy_url}")).ok()?
    } else {
        Url::parse(proxy_url).ok().filter(|u| u.path() == PROXY_PREFIX)?
    };

    parsed
        .query_pairs()
        .find(|(k, _)| k == "url")
        .map(|(_, v)| v.to_string())
}

fn referer_for_url(url: &str) -> Result<String, String> {
    let parsed = Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    Ok(format!("{}/", parsed.origin().ascii_serialization()))
}

async fn fetch_image(client: &reqwest::Client, url: &str) -> Result<bytes::Bytes, String> {
    let referer = referer_for_url(url)?;
    let response = client
        .get(url)
        .header(header::USER_AGENT, USER_AGENT)
        .header(header::REFERER, referer)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("upstream returned HTTP {status}"));
    }

    response.bytes().await.map_err(|e| format!("read body failed: {e}"))
}

pub async fn image_proxy(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ImageProxyParams>,
) -> Response {
    let (url, sig) = match (params.url.as_deref(), params.sig.as_deref()) {
        (Some(url), Some(sig)) if !url.is_empty() && !sig.is_empty() => (url, sig),
        _ => return (StatusCode::BAD_REQUEST, "Bad Request").into_response(),
    };

    if !verify_signature(&state.image_proxy_key, url, sig) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }

    // TODO(security): consider adding session auth like host shell
    match fetch_image(&state.http_client, url).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(axum::body::Body::from(bytes))
            .unwrap_or_else(|e| {
                warn!(error = %e, "image proxy response build failed");
                (StatusCode::BAD_GATEWAY, "Upstream error").into_response()
            }),
        Err(e) => {
            warn!(error = %e, url = %url, "image proxy fetch failed");
            (StatusCode::BAD_GATEWAY, "Upstream error").into_response()
        }
    }
}
