use axum::{
    Json,
    extract::FromRequestParts,
    http::{HeaderMap, StatusCode, request::Parts},
};
use serde_json::{Value, json};
use tokimo_bus_auth::TokimoUser;

/// Authenticated user context extracted from request headers.
#[derive(Debug, Clone)]
pub struct SessionAuth {
    pub user_id: String,
    pub session_id: String,
}

/// Axum extractor — delegates user_id extraction to `TokimoUser` (tokimo-bus-auth)
/// and additionally extracts the optional `x-tokimo-session-id` header.
pub struct AuthUser(pub SessionAuth);

impl std::ops::Deref for AuthUser {
    type Target = SessionAuth;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TokimoUser { user_id } = TokimoUser::from_request_parts(parts, state).await?;
        let session_id = parts
            .headers
            .get("x-tokimo-session-id")
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty())
            .unwrap_or_default()
            .to_owned();
        Ok(AuthUser(SessionAuth { user_id, session_id }))
    }
}

fn _extract_header(headers: &HeaderMap, name: &str) -> Result<String, (StatusCode, Json<Value>)> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, Json(json!({"error": "not authenticated"}))))
}
