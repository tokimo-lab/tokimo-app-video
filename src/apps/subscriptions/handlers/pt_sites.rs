use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

use crate::AppState;
use crate::apps::subscriptions::models::pt_site::{AvailableSiteDto, PtSiteDto, PtSiteStatusDto, PtUserInfoDto};
use crate::apps::subscriptions::repos::pt_site_repo::{CreatePtSiteInput, PtSiteRepo, ReorderItem, UpdatePtSiteInput};
use crate::apps::subscriptions::services::pt_user_info;
use crate::error::{AppError, OptionExt};
use crate::handlers::{ok, user::AuthUser};

// ── Available sites (static registry) ─────────────────────────────────────────

struct SiteInfo {
    id: &'static str,
    name: &'static str,
    domain: &'static str,
    allow_auth_type: &'static [&'static str],
    adult_only: bool,
}

const AVAILABLE_SITES: &[SiteInfo] = &[
    SiteInfo { id: "acgrip", name: "acg", domain: "https://acg.rip", allow_auth_type: &["none"], adult_only: false },
    SiteInfo { id: "agsv", name: "末日", domain: "https://www.agsvpt.com/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "audiences", name: "观众", domain: "https://audiences.me/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "azusa", name: "梓喵", domain: "https://azusa.wiki/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "btschool", name: "学校", domain: "https://pt.btschool.club/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "chdbits", name: "彩虹岛", domain: "https://ptchdbits.co/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "exoticaz", name: "exoticaz", domain: "https://exoticaz.to/", allow_auth_type: &["cookies"], adult_only: true },
    SiteInfo { id: "filelist", name: "filelist", domain: "https://filelist.io/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "hares", name: "白兔", domain: "https://club.hares.top/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "hdatmos", name: "阿童木", domain: "https://hdatmos.club/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "hddolby", name: "高清杜比", domain: "https://www.hddolby.com/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "hdfans", name: "红豆饭", domain: "https://hdfans.org/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "HDHome", name: "家园", domain: "https://hdhome.org/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "hdsky", name: "天空", domain: "https://hdsky.me/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "hhan", name: "憨憨", domain: "https://hhanclub.top/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "iptorrents", name: "iptorrents", domain: "https://iptorrents.com/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "keepfrds", name: "朋友", domain: "https://pt.keepfrds.com/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "m-team", name: "馒头", domain: "https://api.m-team.cc/", allow_auth_type: &["api_key"], adult_only: false },
    SiteInfo { id: "mikanani", name: "蜜柑", domain: "https://mikanani.me", allow_auth_type: &["none"], adult_only: false },
    SiteInfo { id: "ourbits", name: "我堡", domain: "https://ourbits.club/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "pterclub", name: "猫站", domain: "https://pterclub.com/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "ptsbao", name: "烧包乐园", domain: "https://ptsbao.club/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "putao", name: "葡萄", domain: "https://pt.sjtu.edu.cn/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "ssd", name: "不可说", domain: "https://springsunday.net/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "sukebei", name: "Sukebei", domain: "https://sukebei.nyaa.si/", allow_auth_type: &["none"], adult_only: true },
    SiteInfo { id: "tjupt", name: "北洋园", domain: "https://tjupt.org/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "ttg", name: "听听歌", domain: "https://totheglory.im/", allow_auth_type: &["cookies"], adult_only: false },
    SiteInfo { id: "ultrahd", name: "ultrahd", domain: "https://ultrahd.net/", allow_auth_type: &["cookies"], adult_only: false },
];

fn get_available_sites() -> Vec<AvailableSiteDto> {
    AVAILABLE_SITES
        .iter()
        .map(|s| AvailableSiteDto {
            id: s.id.to_string(),
            name: s.name.to_string(),
            domain: s.domain.to_string(),
            allow_auth_type: s.allow_auth_type.iter().map(std::string::ToString::to_string).collect(),
            has_adult_content: s.adult_only,
            adult_only: s.adult_only,
        })
        .collect()
}

fn resolve_domain(site_id: &str) -> Option<String> {
    AVAILABLE_SITES
        .iter()
        .find(|s| s.id == site_id)
        .map(|s| s.domain.to_string())
}

#[derive(Serialize)]
struct SuccessResponse {
    success: bool,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn list_sites(State(state): State<Arc<AppState>>, _auth: AuthUser) -> Response {
    match PtSiteRepo::list(&state.db).await {
        Ok(sites) => ok(sites).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn get_available_sites_handler(_state: State<Arc<AppState>>, _auth: AuthUser) -> Response {
    ok(get_available_sites()).into_response()
}

pub async fn list_with_status(State(state): State<Arc<AppState>>, _auth: AuthUser) -> Response {
    match get_all_site_statuses(&state).await {
        Ok(statuses) => ok(statuses).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn get_site(State(state): State<Arc<AppState>>, _auth: AuthUser, Path(id): Path<String>) -> Response {
    match PtSiteRepo::get_by_id(&state.db, &id).await {
        Ok(Some(site)) => ok(site).into_response(),
        Ok(None) => AppError::NotFound("PT 站点不存在".into()).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn get_site_status(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Response {
    match check_site_status(&state, &id).await {
        Ok(status) => ok(status).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn create_site(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(input): Json<CreatePtSiteInput>,
) -> Response {
    match PtSiteRepo::get_by_site_id(&state.db, &input.site_id).await {
        Ok(Some(_)) => {
            return AppError::Conflict(format!("站点标识 \"{}\" 已存在", input.site_id)).into_response();
        }
        Err(e) => return e.into_response(),
        _ => {}
    }

    let domain = match &input.domain {
        Some(d) if !d.is_empty() => d.clone(),
        _ => match resolve_domain(&input.site_id) {
            Some(d) => d,
            None => {
                return AppError::BadRequest("无法获取站点域名，请检查站点注册表".into()).into_response();
            }
        },
    };

    match PtSiteRepo::create(&state.db, input, &domain).await {
        Ok(site) => ok(site).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn update_site(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<String>,
    Json(input): Json<UpdatePtSiteInput>,
) -> Response {
    match PtSiteRepo::update(&state.db, &id, input).await {
        Ok(site) => ok(site).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn delete_site(State(state): State<Arc<AppState>>, _auth: AuthUser, Path(id): Path<String>) -> Response {
    match PtSiteRepo::delete(&state.db, &id).await {
        Ok(()) => ok(SuccessResponse { success: true }).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn reorder_sites(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(items): Json<Vec<ReorderItem>>,
) -> Response {
    match PtSiteRepo::reorder(&state.db, items).await {
        Ok(()) => ok(SuccessResponse { success: true }).into_response(),
        Err(e) => e.into_response(),
    }
}

// ── Status helpers ─────────────────────────────────────────────────────────────

async fn check_site_status(state: &AppState, id: &str) -> Result<PtSiteStatusDto, AppError> {
    let site = PtSiteRepo::get_by_id(&state.db, id).await?.not_found("PT 站点不存在")?;

    let mut result = PtSiteStatusDto {
        id: site.id.clone(),
        name: site.name.clone(),
        site_id: site.site_id.clone(),
        is_logged_in: false,
        user_info: None,
        last_checked_at: site.last_checked_at.clone(),
        error_message: None,
    };

    let has_creds = site.cookies.is_some() || site.api_key.is_some();
    if site.auth_type == "none" || has_creds {
        if site.auth_type == "api_key" && site.api_key.is_some() {
            match pt_user_info::fetch_user_info(&site).await {
                Ok(info) => {
                    result.is_logged_in = true;
                    result.user_info = Some(info);
                    let _ = PtSiteRepo::update_last_checked(&state.db, id).await;
                }
                Err(e) => {
                    result.error_message = Some(e);
                }
            }
        } else {
            match test_site_connection(&site).await {
                Ok(true) => {
                    result.is_logged_in = true;
                    result.user_info = Some(PtUserInfoDto::empty());
                    let _ = PtSiteRepo::update_last_checked(&state.db, id).await;
                }
                Ok(false) => {
                    result.error_message = Some("连接失败".to_string());
                }
                Err(e) => {
                    result.error_message = Some(e);
                }
            }
        }
    } else {
        result.error_message = Some("缺少站点配置或凭据".to_string());
    }

    Ok(result)
}

async fn get_all_site_statuses(state: &AppState) -> Result<Vec<PtSiteStatusDto>, AppError> {
    let sites = PtSiteRepo::list(&state.db).await?;
    let mut results = Vec::with_capacity(sites.len());
    for site in &sites {
        match check_site_status(state, &site.id).await {
            Ok(status) => results.push(status),
            Err(_) => results.push(PtSiteStatusDto {
                id: site.id.clone(),
                name: site.name.clone(),
                site_id: site.site_id.clone(),
                is_logged_in: false,
                user_info: None,
                last_checked_at: site.last_checked_at.clone(),
                error_message: Some("获取状态失败".to_string()),
            }),
        }
    }
    Ok(results)
}

async fn test_site_connection(site: &PtSiteDto) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.get(&site.domain);
    if let Some(cookies) = &site.cookies {
        req = req.header("Cookie", cookies);
    }
    if let Some(api_key) = &site.api_key {
        req = req.header("x-api-key", api_key);
    }

    match req.send().await {
        Ok(resp) => Ok(resp.status().is_success()),
        Err(e) => Err(e.to_string()),
    }
}
