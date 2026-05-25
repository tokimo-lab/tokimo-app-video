use chrono::Utc;
use sea_orm::{sea_query::Expr, *};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::AppState;
use crate::bus_clients::jobs::{self as jobs_client, CreateJobRequest, UpdateStatusRequest};
use crate::db::entities::jobs;
use crate::error::AppError;

pub struct JobRepo;

impl JobRepo {
    /// Create a new job record with status "pending".
    pub async fn create_job(
        db: &DatabaseConnection,
        job_type: &str,
        params: JsonValue,
        data: Option<JsonValue>,
        user_id: Option<Uuid>,
    ) -> Result<jobs::Model, AppError> {
        let now = Utc::now().fixed_offset();
        let model = jobs::ActiveModel {
            id: Set(Uuid::new_v4()),
            r#type: Set(job_type.to_string()),
            status: Set("pending".to_string()),
            user_id: Set(user_id),
            parent_job_id: Set(None),
            task_type: Set(None),
            params: Set(params),
            data: Set(data),
            progress: Set(0),
            retry_count: Set(0),
            max_retries: Set(3),
            error: Set(None),
            started_at: Set(None),
            completed_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            dedupe_key: Set(None),
            alias_job_id: Set(None),
            priority: Set(crate::queue::JobPriority::Normal.as_i32()),
        };
        Ok(jobs::Entity::insert(model).exec_with_returning(db).await?)
    }

    /// Update job progress and data. Fallback when bus is unavailable.
    pub async fn update_progress(
        db: &DatabaseConnection,
        id: Uuid,
        progress: i32,
        data: Option<JsonValue>,
    ) -> Result<Option<jobs::Model>, AppError> {
        let mut update = jobs::Entity::update_many()
            .col_expr(jobs::Column::Progress, Expr::value(progress))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id));

        if let Some(m) = data {
            update = update.col_expr(jobs::Column::Data, Expr::value(m));
        }

        let result = update.exec(db).await?;
        if result.rows_affected == 0 {
            return Ok(None);
        }
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    pub async fn create_job_via_bus(
        state: &AppState,
        job_type: &str,
        params: JsonValue,
        data: Option<JsonValue>,
        user_id: Option<Uuid>,
    ) -> Result<jobs::Model, AppError> {
        let Some(client) = state.bus_client.get() else {
            return Self::create_job(&state.db, job_type, params, data, user_id).await;
        };
        let Some(caller_user_id) = user_id else {
            return Err(AppError::Unauthorized(
                "jobs.create via bus requires caller user_id".into(),
            ));
        };
        let request = CreateJobRequest::new(job_type, params).with_data(data);
        jobs_client::create(client, jobs_client::video_caller(Some(caller_user_id)), request).await
    }

    pub async fn update_progress_via_bus(
        state: &AppState,
        job_id: Uuid,
        progress: i32,
        data: Option<JsonValue>,
        user_id: Option<Uuid>,
    ) -> Result<Option<jobs::Model>, AppError> {
        let Some(client) = state.bus_client.get() else {
            return Self::update_progress(&state.db, job_id, progress, data).await;
        };
        let request = UpdateStatusRequest {
            job_id,
            status: "running".to_string(),
            error: None,
            result: data,
            progress: Some(progress),
        };
        jobs_client::update_status(client, jobs_client::video_caller(user_id), request)
            .await
            .map(Some)
    }
}
