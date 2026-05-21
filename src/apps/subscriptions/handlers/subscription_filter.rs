use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;
use crate::apps::subscriptions::repos::subscription_filter_repo::{
    CreateSubscriptionFilterInput, ReorderItem, SubscriptionFilterRepo, UpdateSubscriptionFilterInput,
};
use crate::error::AppError;
use crate::handlers::{ok, user::AuthUser};

#[derive(Serialize)]
struct SuccessResponse {
    success: bool,
}

fn check_ownership(owner: Option<&str>, user_id: &str) -> Result<(), Response> {
    if owner.is_some_and(|o| o != user_id) {
        Err(AppError::Forbidden("无权访问此资源".into()).into_response())
    } else {
        Ok(())
    }
}

pub async fn list(State(state): State<Arc<AppState>>, auth: AuthUser) -> Response {
    match SubscriptionFilterRepo::list(&state.db, &auth.user_id).await {
        Ok(filters) => ok(filters).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn get_by_id(State(state): State<Arc<AppState>>, auth: AuthUser, Path(id): Path<String>) -> Response {
    match SubscriptionFilterRepo::get_by_id(&state.db, &id).await {
        Ok(Some(filter)) => {
            if let Err(e) = check_ownership(filter.created_by.as_deref(), &auth.user_id) {
                return e;
            }
            ok(filter).into_response()
        }
        Ok(None) => AppError::NotFound("过滤规则不存在".into()).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(input): Json<CreateSubscriptionFilterInput>,
) -> Response {
    match SubscriptionFilterRepo::create(&state.db, input, &auth.user_id).await {
        Ok(filter) => ok(filter).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(input): Json<UpdateSubscriptionFilterInput>,
) -> Response {
    match SubscriptionFilterRepo::get_raw_by_id(&state.db, &id).await {
        Ok(Some(existing)) => {
            if let Err(e) = check_ownership(
                existing.created_by.map(|u| u.to_string()).as_deref(),
                &auth.user_id,
            ) {
                return e;
            }
        }
        Ok(None) => return AppError::NotFound("过滤规则不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    }

    match SubscriptionFilterRepo::update(&state.db, &id, input).await {
        Ok(Some(filter)) => ok(filter).into_response(),
        Ok(None) => AppError::NotFound("过滤规则不存在".into()).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn delete(State(state): State<Arc<AppState>>, auth: AuthUser, Path(id): Path<String>) -> Response {
    match SubscriptionFilterRepo::get_raw_by_id(&state.db, &id).await {
        Ok(Some(existing)) => {
            if let Err(e) = check_ownership(
                existing.created_by.map(|u| u.to_string()).as_deref(),
                &auth.user_id,
            ) {
                return e;
            }
        }
        Ok(None) => return AppError::NotFound("过滤规则不存在".into()).into_response(),
        Err(e) => return e.into_response(),
    }

    match SubscriptionFilterRepo::delete(&state.db, &id).await {
        Ok(_) => ok(SuccessResponse { success: true }).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn reorder(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(orders): Json<Vec<ReorderItem>>,
) -> Response {
    match SubscriptionFilterRepo::reorder(&state.db, orders).await {
        Ok(()) => ok(SuccessResponse { success: true }).into_response(),
        Err(e) => e.into_response(),
    }
}
