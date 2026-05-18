use axum::{
    Json,
    extract::FromRequestParts,
    http::{HeaderMap, StatusCode, request::Parts},
};
use serde_json::{Value, json};

/// Authenticated user context extracted from request headers.
#[derive(Debug, Clone)]
pub struct SessionAuth {
    pub user_id: String,
    pub session_id: String,
}

/// Axum extractor that provides `SessionAuth` from headers.
pub struct AuthUser(pub SessionAuth);

impl std::ops::Deref for AuthUser {
    type Target = SessionAuth;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for AuthUser
where S: Send + Sync {
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;
        let user_id = extract_header(headers, "x-tokimo-user-id")?;
        let session_id = extract_header(headers, "x-tokimo-session-id").unwrap_or_default();
        Ok(AuthUser(SessionAuth { user_id, session_id }))
    }
}

fn extract_header(headers: &HeaderMap, name: &str) -> Result<String, (StatusCode, Json<Value>)> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, Json(json!({"error": "not authenticated"}))))
}
