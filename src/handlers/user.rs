use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use serde_json::{Value, json};

/// Authenticated user extracted from `X-Tokimo-User-Id` header.
#[derive(Debug, Clone)]
pub struct AuthUser(pub tokimo_bus_auth::TokimoUser);

impl<S> FromRequestParts<S> for AuthUser
where S: Send + Sync {
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = tokimo_bus_auth::TokimoUser::from_request_parts(parts, state).await?;
        Ok(Self(user))
    }
}
