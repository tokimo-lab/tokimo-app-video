use sea_orm::*;
use tracing::error;
use uuid::Uuid;

use crate::config::SystemSettings;
use crate::db::entities::{sessions, users};
use crate::db::repos::system_config_repo::SystemConfigRepo;
use crate::error::AppError;
pub struct AuthRepo;

impl AuthRepo {
    /// Validate a user session by `SESSION_ID` cookie value.
    pub async fn validate_session(db: &DatabaseConnection, session_id: &str) -> Result<bool, AppError> {
        let uid: Uuid = session_id
            .parse()
            .map_err(|_| AppError::Unauthorized("invalid session id".into()))?;
        let exists = sessions::Entity::find_by_id(uid)
            .filter(sessions::Column::ExpiresAt.gt(chrono::Utc::now()))
            .one(db)
            .await
            .map_err(|err| {
                error!("session lookup failed: {}", err);
                AppError::Internal("Session validation failed".into())
            })?;
        Ok(exists.is_some())
    }

    /// 从有效 session 中获取 `user_id。失效或不存在返回` None。
    pub async fn get_user_id_by_session(db: &DatabaseConnection, session_id: &str) -> Result<Option<String>, AppError> {
        let uid: Uuid = session_id
            .parse()
            .map_err(|_| AppError::Unauthorized("invalid session id".into()))?;
        let row = sessions::Entity::find_by_id(uid)
            .filter(sessions::Column::ExpiresAt.gt(chrono::Utc::now()))
            .one(db)
            .await
            .map_err(|err| {
                error!("get_user_id_by_session failed: {}", err);
                AppError::Internal("Session lookup failed".into())
            })?;
        Ok(row.map(|r| r.user_id.to_string()))
    }

    /// Validate an internal stream access token.
    pub async fn validate_internal_stream_token(db: &DatabaseConnection, access_token: &str) -> Result<bool, AppError> {
        let settings = SystemConfigRepo::get::<SystemSettings>(db).await.map_err(|err| {
            error!("internal stream token lookup failed: {}", err);
            AppError::Internal("Internal token validation failed".into())
        })?;
        let valid = match (
            &settings.internal_stream_access_token,
            &settings.internal_stream_access_token_expires_at,
        ) {
            (Some(token), Some(expires_at)) => token == access_token && *expires_at > chrono::Utc::now(),
            _ => false,
        };
        Ok(valid)
    }

    /// 按邮箱查找用户（登录用）。
    pub async fn find_user_by_email(db: &DatabaseConnection, email: &str) -> Result<Option<users::Model>, AppError> {
        let row = users::Entity::find()
            .filter(users::Column::Email.eq(email))
            .one(db)
            .await?;
        Ok(row)
    }

    /// 创建新 session（登录成功后调用），返回 `session_id` 字符串。
    pub async fn create_session(
        db: &DatabaseConnection,
        user_id: Uuid,
        user_agent: Option<&str>,
    ) -> Result<String, AppError> {
        let session_id = Uuid::new_v4();
        let expires_at = (chrono::Utc::now() + chrono::Duration::days(7)).into();
        let (browser, browser_version, os) = parse_user_agent(user_agent);

        let active = sessions::ActiveModel {
            id: Set(session_id),
            user_id: Set(user_id),
            expires_at: Set(expires_at),
            user_agent: Set(user_agent.map(std::string::ToString::to_string)),
            browser: Set(browser),
            browser_version: Set(browser_version),
            os: Set(os),
            created_at: Set(Some(chrono::Utc::now().into())),
        };
        sessions::Entity::insert(active).exec(db).await?;
        Ok(session_id.to_string())
    }

    /// 删除 session（登出时调用）。
    pub async fn delete_session(db: &DatabaseConnection, session_id: &str) -> Result<(), AppError> {
        let sid: Uuid = session_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid session id".into()))?;
        sessions::Entity::delete_by_id(sid).exec(db).await?;
        Ok(())
    }

    /// 更新用户最后登录时间。
    pub async fn update_last_login(db: &DatabaseConnection, user_id: Uuid) -> Result<(), AppError> {
        let model = users::Entity::find_by_id(user_id).one(db).await?;
        if let Some(m) = model {
            let mut active: users::ActiveModel = m.into();
            active.last_login_at = Set(Some(chrono::Utc::now().into()));
            active.update(db).await?;
        }
        Ok(())
    }

    /// 统计用户数量（判断是否为首次启动）。
    pub async fn count_users(db: &DatabaseConnection) -> Result<u64, AppError> {
        let count = users::Entity::find().count(db).await?;
        Ok(count)
    }

    /// Check whether a user has registered any passkeys.
    pub async fn has_passkeys(db: &DatabaseConnection, user_id: Uuid) -> Result<bool, AppError> {
        use crate::db::entities::pass_keys;
        let count = pass_keys::Entity::find()
            .filter(pass_keys::Column::UserId.eq(user_id))
            .count(db)
            .await?;
        Ok(count > 0)
    }
}

/// 简易 User-Agent 解析，识别浏览器和操作系统。
fn parse_user_agent(ua: Option<&str>) -> (Option<String>, Option<String>, Option<String>) {
    let ua = match ua {
        Some(s) if !s.is_empty() => s,
        _ => return (None, None, None),
    };

    let (browser, version) = if ua.contains("Edg/") {
        ("Edge", extract_version(ua, "Edg/"))
    } else if ua.contains("OPR/") {
        ("Opera", extract_version(ua, "OPR/"))
    } else if ua.contains("Chrome/") && !ua.contains("Chromium/") {
        ("Chrome", extract_version(ua, "Chrome/"))
    } else if ua.contains("Firefox/") {
        ("Firefox", extract_version(ua, "Firefox/"))
    } else if ua.contains("Safari/") && ua.contains("Version/") {
        ("Safari", extract_version(ua, "Version/"))
    } else {
        ("Unknown", None)
    };

    let os = if ua.contains("Windows NT") {
        "Windows"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("iPhone") || ua.contains("iPad") {
        "iOS"
    } else if ua.contains("Mac OS X") {
        "macOS"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "Unknown"
    };

    (Some(browser.to_string()), version, Some(os.to_string()))
}

fn extract_version(ua: &str, prefix: &str) -> Option<String> {
    ua.split(prefix)
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .map(|s| s.trim_end_matches(';').to_string())
        .filter(|s| !s.is_empty())
}
