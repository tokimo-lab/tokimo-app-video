use chrono::Utc;
use sea_orm::{sea_query::Expr, *};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::db::entities::jobs;
use crate::db::models::job::{JobOutput, JobStatsOutput};
use crate::db::pagination::{Page, PageInput};
use crate::error::AppError;
use crate::queue::cancellation::CANCEL_MARKER_ABORTED;

/// Tuple of (job_type, payload, meta, user_id, parent_id, t_type) used by
/// [`JobRepo::create_child_jobs_batch`] to insert many child jobs in one call.
pub type ChildJobData<'a> = (&'a str, JsonValue, Option<JsonValue>, Option<Uuid>, Uuid, String);

/// Free-form filter shared by the task-queue listing endpoints and the
/// filter-aware bulk operations (suspend / resume / abort / cleanup).
///
/// * `type_` — job type exact match.
/// * `status` — status exact match. Honoured by listings and by
///   `cleanup_all_finished_by_filter`; **ignored** by suspend/resume/abort
///   bulk ops because those operations redefine the target status set
///   themselves.
/// * `search` — substring match against the `type` column (case-insensitive).
#[derive(Debug, Default, Clone)]
pub struct JobListFilter {
    pub type_: Option<String>,
    pub status: Option<String>,
    pub search: Option<String>,
}

impl JobListFilter {
    /// Apply `type` + `search` (and optionally `status`) to any query that
    /// implements [`QueryFilter`] (both `Select<Entity>` and `UpdateMany<E>`
    /// / `DeleteMany<E>` fit).
    fn apply<Q: QueryFilter>(&self, mut q: Q, include_status: bool) -> Q {
        if let Some(t) = &self.type_ {
            q = q.filter(jobs::Column::Type.eq(t.as_str()));
        }
        if include_status && let Some(s) = &self.status {
            q = q.filter(jobs::Column::Status.eq(s.as_str()));
        }
        if let Some(s) = &self.search
            && !s.is_empty()
        {
            q = q.filter(jobs::Column::Type.contains(s));
        }
        q
    }
}

pub struct JobRepo;

/// Aggregated child-job state for a parent job.
#[derive(Debug, Clone, Copy)]
pub struct ParentAggregate {
    pub total_children: i32,
    pub done: i32,
    pub successes: i32,
    pub failures: i32,
    pub progress: i32,
    pub completed: bool,
}

impl JobRepo {
    /// Create a new job record with status "pending".
    pub async fn create_job(
        db: &DatabaseConnection,
        job_type: &str,
        payload: JsonValue,
        meta: Option<JsonValue>,
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
            payload: Set(payload),
            meta: Set(meta),
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

    /// Create a new job with full options (parent, task type, dedupe, priority, alias).
    ///
    /// Used by the high-level [`enqueue_with_dedupe`](Self::enqueue_with_dedupe)
    /// path; callers needing only the simple shape should keep using
    /// [`create_job`](Self::create_job).
    #[allow(clippy::too_many_arguments)]
    pub async fn create_job_with_options<C: ConnectionTrait>(
        conn: &C,
        job_type: &str,
        payload: JsonValue,
        meta: Option<JsonValue>,
        user_id: Option<Uuid>,
        parent_job_id: Option<Uuid>,
        task_type: Option<String>,
        dedupe_key: Option<String>,
        alias_job_id: Option<Uuid>,
        priority: i32,
        initial_status: &str,
    ) -> Result<jobs::Model, AppError> {
        let now = Utc::now().fixed_offset();
        let model = jobs::ActiveModel {
            id: Set(Uuid::new_v4()),
            r#type: Set(job_type.to_string()),
            status: Set(initial_status.to_string()),
            user_id: Set(user_id),
            parent_job_id: Set(parent_job_id),
            task_type: Set(task_type),
            payload: Set(payload),
            meta: Set(meta),
            progress: Set(0),
            retry_count: Set(0),
            max_retries: Set(3),
            error: Set(None),
            started_at: Set(None),
            completed_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            dedupe_key: Set(dedupe_key),
            alias_job_id: Set(alias_job_id),
            priority: Set(priority),
        };
        Ok(jobs::Entity::insert(model).exec_with_returning(conn).await?)
    }

    /// Find an active (non-terminal) leader job that matches `(job_type, dedupe_key)`.
    ///
    /// Returns the leader (`alias_job_id IS NULL`) only — alias rows are
    /// excluded so callers never form chained aliases (A→B→C).
    pub async fn find_active_by_dedupe<C: ConnectionTrait>(
        conn: &C,
        job_type: &str,
        dedupe_key: &str,
    ) -> Result<Option<jobs::Model>, AppError> {
        Ok(jobs::Entity::find()
            .filter(jobs::Column::Type.eq(job_type))
            .filter(jobs::Column::DedupeKey.eq(dedupe_key))
            .filter(jobs::Column::AliasJobId.is_null())
            .filter(jobs::Column::Status.is_in(["pending", "running", "waiting", "suspended"]))
            .order_by_asc(jobs::Column::CreatedAt)
            .one(conn)
            .await?)
    }

    /// Enqueue a job with optional `dedupe_key`. If a leader with the same
    /// `(job_type, dedupe_key)` is already active, the new row is inserted
    /// as an **alias** whose `status` is initialised from the target so it
    /// never enters the scheduler's pending set unnecessarily:
    ///
    /// * target `running`/`waiting` → alias `running` (state-bound, copies progress/meta)
    /// * target `pending` → alias `pending` (joins the dispatch group)
    /// * target terminal (`succeeded`/`failed`/`cancelled`) → alias mirrors the same
    ///   status and copies `error`/`meta`/`completed_at` for visibility
    ///
    /// Returns `(inserted_job, target_if_alias)` — `target_if_alias` is
    /// `Some(leader)` when this enqueue created an alias, `None` when the
    /// new row stands on its own (becomes the leader).
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue_with_dedupe(
        db: &DatabaseConnection,
        job_type: &str,
        payload: JsonValue,
        meta: Option<JsonValue>,
        user_id: Option<Uuid>,
        parent_job_id: Option<Uuid>,
        task_type: Option<String>,
        dedupe_key: Option<String>,
        priority: i32,
    ) -> Result<(jobs::Model, Option<jobs::Model>), AppError> {
        let txn = db.begin().await?;

        let leader = if let Some(key) = dedupe_key.as_deref() {
            Self::find_active_by_dedupe(&txn, job_type, key).await?
        } else {
            None
        };

        let (initial_status, alias_id, alias_meta, alias_error) = match &leader {
            Some(t) => {
                let status = match t.status.as_str() {
                    "running" | "waiting" => "running",
                    "pending" => "pending",
                    other => other, // succeeded / failed / cancelled / suspended → mirror
                };
                (status.to_string(), Some(t.id), t.meta.clone(), t.error.clone())
            }
            None => ("pending".to_string(), None, meta.clone(), None),
        };

        // For alias rows we prefer to inherit the leader's progress meta so the
        // user immediately sees the in-flight result; for new leaders the caller's
        // `meta` wins.
        let final_meta = if leader.is_some() { alias_meta } else { meta };

        let inserted = Self::create_job_with_options(
            &txn,
            job_type,
            payload,
            final_meta,
            user_id,
            parent_job_id,
            task_type,
            dedupe_key,
            alias_id,
            priority,
            &initial_status,
        )
        .await?;

        // Mirror leader's terminal-state error/progress on the alias row.
        if let (Some(t), true) = (
            &leader,
            alias_error.is_some() || matches!(inserted.status.as_str(), "succeeded" | "failed" | "cancelled"),
        ) {
            let mut am: jobs::ActiveModel = inserted.clone().into();
            am.error = Set(t.error.clone());
            am.progress = Set(t.progress);
            am.completed_at = Set(t.completed_at);
            let _ = am.update(&txn).await?;
        }

        txn.commit().await?;
        Ok((inserted, leader))
    }
    /// Batch create multiple jobs efficiently.
    /// Each tuple is `(job_type, payload, meta, user_id)`.
    pub async fn create_jobs_batch(
        db: &DatabaseConnection,
        jobs_data: Vec<(&str, JsonValue, Option<JsonValue>, Option<Uuid>)>,
    ) -> Result<u64, AppError> {
        if jobs_data.is_empty() {
            return Ok(0);
        }
        let now = Utc::now().fixed_offset();
        let models: Vec<jobs::ActiveModel> = jobs_data
            .into_iter()
            .map(|(job_type, payload, meta, user_id)| jobs::ActiveModel {
                id: Set(Uuid::new_v4()),
                r#type: Set(job_type.to_string()),
                status: Set("pending".to_string()),
                user_id: Set(user_id),
                parent_job_id: Set(None),
                task_type: Set(None),
                payload: Set(payload),
                meta: Set(meta),
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
            })
            .collect();
        let count = models.len() as u64;
        // Postgres caps at 65535 bind params per statement. jobs has 13
        // bound columns per row, so ~5000 rows fit — cap at 4000 for
        // safety under the RC.
        const CHUNK: usize = 4000;
        for chunk in models.chunks(CHUNK) {
            jobs::Entity::insert_many(chunk.to_vec()).exec(db).await?;
        }
        Ok(count)
    }

    /// Batch create child jobs tied to a parent. Sets both the dedicated
    /// `parent_job_id` and `task_type` columns (first-class relationship
    /// fields) so queries can use indexes and don't rely on JSONB `meta`
    /// which `mark_completed` may replace.
    pub async fn create_child_jobs_batch(
        db: &DatabaseConnection,
        jobs_data: Vec<ChildJobData<'_>>,
    ) -> Result<u64, AppError> {
        if jobs_data.is_empty() {
            return Ok(0);
        }
        let now = Utc::now().fixed_offset();
        let models: Vec<jobs::ActiveModel> = jobs_data
            .into_iter()
            .map(
                |(job_type, payload, meta, user_id, parent_id, t_type)| jobs::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    r#type: Set(job_type.to_string()),
                    status: Set("pending".to_string()),
                    user_id: Set(user_id),
                    parent_job_id: Set(Some(parent_id)),
                    task_type: Set(Some(t_type)),
                    payload: Set(payload),
                    meta: Set(meta),
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
                },
            )
            .collect();
        let count = models.len() as u64;
        const CHUNK: usize = 4000;
        for chunk in models.chunks(CHUNK) {
            jobs::Entity::insert_many(chunk.to_vec()).exec(db).await?;
        }
        Ok(count)
    }

    pub async fn count_pending(db: &DatabaseConnection, job_type: &str) -> Result<u64, AppError> {
        let count = jobs::Entity::find()
            .filter(jobs::Column::Type.eq(job_type))
            .filter(jobs::Column::Status.eq("pending"))
            .count(db)
            .await?;
        Ok(count)
    }

    /// Update job status. Automatically sets `started_at` when moving to
    /// "running" and `completed_at` when moving to "completed" or "failed".
    pub async fn update_status(
        db: &DatabaseConnection,
        id: Uuid,
        status: &str,
        error: Option<String>,
    ) -> Result<(), AppError> {
        let mut update = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value(status))
            .col_expr(jobs::Column::Error, Expr::value(error))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id));

        if status == "running" {
            update = update.col_expr(jobs::Column::StartedAt, Expr::cust("NOW()"));
        }
        if status == "completed" || status == "failed" {
            update = update.col_expr(jobs::Column::CompletedAt, Expr::cust("NOW()"));
        }

        let result = update.exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("job {id} not found")));
        }
        Ok(())
    }

    /// List jobs with optional filtering and pagination.
    pub async fn list_jobs(
        db: &DatabaseConnection,
        job_type: Option<&str>,
        status: Option<&str>,
        page: &PageInput,
    ) -> Result<Page<JobOutput>, AppError> {
        let mut query = jobs::Entity::find().order_by_desc(jobs::Column::CreatedAt);

        if let Some(t) = job_type {
            query = query.filter(jobs::Column::Type.eq(t));
        }
        if let Some(s) = status {
            query = query.filter(jobs::Column::Status.eq(s));
        }

        let total = query.clone().count(db).await? as i64;
        let models = query
            .paginate(db, page.page_size)
            .fetch_page(page.page.saturating_sub(1))
            .await?;

        let child_counts =
            Self::count_children_for_parents(db, &models.iter().map(|m| m.id.to_string()).collect::<Vec<_>>()).await?;

        let items = models
            .into_iter()
            .map(|m| {
                let id = m.id.to_string();
                let mut out = JobOutput::from(m);
                out.child_count = child_counts.get(&id).copied().unwrap_or(0);
                out
            })
            .collect();
        Ok(Page::new(items, total, page))
    }

    /// Batch-count children jobs grouped by parent_job_id column.
    /// Returns a map of parent_id -> child count. Missing parents map to 0 (caller default).
    async fn count_children_for_parents(
        db: &DatabaseConnection,
        parent_ids: &[String],
    ) -> Result<std::collections::HashMap<String, i64>, AppError> {
        let mut map = std::collections::HashMap::new();
        if parent_ids.is_empty() {
            return Ok(map);
        }
        let uuids: Vec<Uuid> = parent_ids.iter().filter_map(|s| Uuid::parse_str(s).ok()).collect();
        if uuids.is_empty() {
            return Ok(map);
        }
        let rows = jobs::Entity::find()
            .select_only()
            .column(jobs::Column::ParentJobId)
            .column_as(jobs::Column::Id.count(), "cnt")
            .filter(jobs::Column::ParentJobId.is_in(uuids))
            .group_by(jobs::Column::ParentJobId)
            .into_tuple::<(Option<Uuid>, i64)>()
            .all(db)
            .await?;
        for (pid, cnt) in rows {
            if let Some(pid) = pid {
                map.insert(pid.to_string(), cnt);
            }
        }
        Ok(map)
    }

    /// Get a single job by ID.
    pub async fn get_by_id(db: &DatabaseConnection, id: Uuid) -> Result<Option<jobs::Model>, AppError> {
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    /// Get aggregated job statistics (counts by status).
    pub async fn stats(db: &DatabaseConnection) -> Result<JobStatsOutput, AppError> {
        let rows = jobs::Entity::find()
            .select_only()
            .column(jobs::Column::Status)
            .column_as(jobs::Column::Id.count(), "cnt")
            .group_by(jobs::Column::Status)
            .into_tuple::<(String, i64)>()
            .all(db)
            .await?;

        let mut stats = JobStatsOutput {
            total: 0,
            pending: 0,
            running: 0,
            completed: 0,
            failed: 0,
            cancelled: 0,
        };

        for (status, cnt) in rows {
            stats.total += cnt;
            match status.as_str() {
                "pending" => stats.pending = cnt,
                "running" => stats.running = cnt,
                "completed" => stats.completed = cnt,
                "failed" => stats.failed = cnt,
                "cancelled" => stats.cancelled = cnt,
                _ => {}
            }
        }

        Ok(stats)
    }

    /// Cancel a pending job (set status to "cancelled").
    /// Cancel a single job by id. Accepts any non-terminal status (pending /
    /// running / waiting / suspended). Running jobs also need cooperative
    /// interruption via `CancellationRegistry` — that part happens in the
    /// handler; this repo only flips DB state.
    pub async fn cancel_job(db: &DatabaseConnection, id: Uuid) -> Result<bool, AppError> {
        let result = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("cancelled"))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NOW()"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            .filter(jobs::Column::Status.is_in(["pending", "running", "waiting", "suspended"]))
            .exec(db)
            .await?;
        Ok(result.rows_affected > 0)
    }

    /// Reset a failed job back to "pending" for retry.
    pub async fn retry_job(db: &DatabaseConnection, id: Uuid) -> Result<Option<jobs::Model>, AppError> {
        let result = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("pending"))
            .col_expr(jobs::Column::Error, Expr::value::<Option<String>>(None))
            .col_expr(jobs::Column::Progress, Expr::value(0))
            .col_expr(jobs::Column::StartedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            .filter(jobs::Column::Status.eq("failed"))
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            return Ok(None);
        }
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    /// Retry all failed jobs (optionally filtered by type).
    pub async fn retry_all_failed(db: &DatabaseConnection, job_type: Option<&str>) -> Result<u64, AppError> {
        let mut update = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("pending"))
            .col_expr(jobs::Column::Error, Expr::cust("NULL"))
            .col_expr(jobs::Column::Progress, Expr::value(0))
            .col_expr(jobs::Column::StartedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Status.eq("failed"));

        if let Some(t) = job_type {
            update = update.filter(jobs::Column::Type.eq(t));
        }

        let result = update.exec(db).await?;
        Ok(result.rows_affected)
    }

    /// Cancel all pending/running jobs whose payload contains a given appId.
    pub async fn cancel_jobs_by_app_id(db: &DatabaseConnection, app_id: Uuid) -> Result<u64, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"UPDATE jobs
               SET status = 'cancelled',
                   completed_at = NOW(),
                   updated_at = NOW()
             WHERE status IN ('pending', 'running')
               AND payload->>'appId' = $1",
            [app_id.to_string().into()],
        );
        let result = db.execute_raw(stmt).await?;
        Ok(result.rows_affected())
    }

    /// Preempt all active parent scan jobs for (`app_id`, `task_type`). Marks
    /// them `cancelled` with the supplied reason written into `jobs.error`.
    /// Targets only top-level parents (`parent_job_id IS NULL`) — children
    /// are cascaded separately via [`cancel_children_of`] so callers can
    /// also signal each child's running worker via the cancellation
    /// registry.
    ///
    /// Returns the ids of all parents actually flipped from an active
    /// status (pending / running / waiting / suspended) to cancelled.
    pub async fn preempt_scans(
        db: &DatabaseConnection,
        app_id: Uuid,
        task_type: &str,
        reason: &str,
    ) -> Result<Vec<Uuid>, AppError> {
        use sea_orm::FromQueryResult;

        #[derive(FromQueryResult)]
        struct IdRow {
            id: Uuid,
        }

        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"UPDATE jobs
                 SET status = 'cancelled',
                     error = $3,
                     completed_at = NOW(),
                     updated_at = NOW()
               WHERE status IN ('pending', 'running', 'waiting', 'suspended')
                 AND type = $2
                 AND parent_job_id IS NULL
                 AND payload->>'appId' = $1
             RETURNING id",
            [app_id.to_string().into(), task_type.into(), reason.into()],
        );
        let rows = IdRow::find_by_statement(stmt).all(db).await?;
        Ok(rows.into_iter().map(|r| r.id).collect())
    }

    /// Cancel any active scan-child job whose payload `photoId` matches the
    /// given photo. Used when the user fires a single-photo "refresh" action
    /// (priority=UserAction) and we want to abort an in-flight scan child for
    /// the same photo so the user job is the sole authority.
    ///
    /// Matches `r#type = job_type` (e.g. `photo_ocr`) AND
    /// `payload->>'photoId' = photo_id` AND status non-terminal AND has a
    /// `parent_job_id` (so we don't accidentally hit a single-photo job).
    /// Returns the ids cancelled.
    pub async fn preempt_scan_child_for(
        db: &DatabaseConnection,
        job_type: &str,
        photo_id: Uuid,
        reason: &str,
    ) -> Result<Vec<Uuid>, AppError> {
        use sea_orm::FromQueryResult;

        #[derive(FromQueryResult)]
        struct IdRow {
            id: Uuid,
        }

        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"UPDATE jobs
                 SET status = 'cancelled',
                     error = $3,
                     completed_at = NOW(),
                     updated_at = NOW()
               WHERE status IN ('pending', 'running', 'waiting', 'suspended')
                 AND type = $1
                 AND parent_job_id IS NOT NULL
                 AND payload->>'photoId' = $2
             RETURNING id",
            [job_type.into(), photo_id.to_string().into(), reason.into()],
        );
        let rows = IdRow::find_by_statement(stmt).all(db).await?;
        Ok(rows.into_iter().map(|r| r.id).collect())
    }

    /// Cascade-cancel all children of the given parent ids that are still in
    /// a non-terminal state. Writes `reason` into `jobs.error`. Returns the
    /// ids of the children actually cancelled (so callers can also signal
    /// their running workers via the cancellation registry).
    pub async fn cancel_children_of(
        db: &DatabaseConnection,
        parent_ids: &[Uuid],
        reason: &str,
    ) -> Result<Vec<Uuid>, AppError> {
        use sea_orm::FromQueryResult;

        if parent_ids.is_empty() {
            return Ok(Vec::new());
        }

        #[derive(FromQueryResult)]
        struct IdRow {
            id: Uuid,
        }

        // Build $3, $4, ... placeholders for the parent id array so we can
        // bind uuids instead of inlining them.
        let placeholders: Vec<String> = (0..parent_ids.len()).map(|i| format!("${}", i + 2)).collect();
        let sql = format!(
            r"UPDATE jobs
                 SET status = 'cancelled',
                     error = $1,
                     completed_at = NOW(),
                     updated_at = NOW()
               WHERE status NOT IN ('completed', 'failed', 'cancelled')
                 AND parent_job_id IN ({})
             RETURNING id",
            placeholders.join(", ")
        );

        let mut values: Vec<sea_orm::Value> = Vec::with_capacity(parent_ids.len() + 1);
        values.push(reason.into());
        for pid in parent_ids {
            values.push((*pid).into());
        }

        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, values);
        let rows = IdRow::find_by_statement(stmt).all(db).await?;
        Ok(rows.into_iter().map(|r| r.id).collect())
    }

    /// Cascade-suspend all children of the given parent ids that are currently
    /// `pending` or `running`. Returns the ids of the children actually
    /// suspended so callers can fire cooperative cancel signals and broadcast
    /// events.
    pub async fn suspend_children_of(db: &DatabaseConnection, parent_ids: &[Uuid]) -> Result<Vec<Uuid>, AppError> {
        use sea_orm::FromQueryResult;

        if parent_ids.is_empty() {
            return Ok(Vec::new());
        }

        #[derive(FromQueryResult)]
        struct IdRow {
            id: Uuid,
        }

        let placeholders: Vec<String> = (0..parent_ids.len()).map(|i| format!("${}", i + 1)).collect();
        let sql = format!(
            r"UPDATE jobs
                 SET status = 'suspended',
                     completed_at = NULL,
                     updated_at = NOW()
               WHERE status IN ('pending', 'running')
                 AND parent_job_id IN ({})
             RETURNING id",
            placeholders.join(", ")
        );

        let mut values: Vec<sea_orm::Value> = Vec::with_capacity(parent_ids.len());
        for pid in parent_ids {
            values.push((*pid).into());
        }

        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, values);
        let rows = IdRow::find_by_statement(stmt).all(db).await?;
        Ok(rows.into_iter().map(|r| r.id).collect())
    }

    /// Return the ids of all children of `parent_id` whose status is
    /// `suspended`. Used by `resume_one` to cascade-resume children that were
    /// cascade-suspended when the parent was suspended.
    pub async fn list_suspended_children_of(db: &DatabaseConnection, parent_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        let models = jobs::Entity::find()
            .select_only()
            .column(jobs::Column::Id)
            .filter(jobs::Column::ParentJobId.eq(parent_id))
            .filter(jobs::Column::Status.eq("suspended"))
            .all(db)
            .await?;
        Ok(models.into_iter().map(|m| m.id).collect())
    }

    /// Delete child jobs orphaned by a prior parent deletion. Any job with
    /// a `parent_job_id` pointing at a no-longer-existing parent is removed.
    /// Called after every delete path that might remove parents.
    pub async fn cleanup_orphan_children(db: &DatabaseConnection) -> Result<u64, AppError> {
        let stmt = Statement::from_string(
            DatabaseBackend::Postgres,
            "DELETE FROM jobs WHERE parent_job_id IS NOT NULL \
               AND NOT EXISTS (SELECT 1 FROM jobs p WHERE p.id = jobs.parent_job_id)"
                .to_string(),
        );
        let result = db.execute_raw(stmt).await?;
        Ok(result.rows_affected())
    }

    /// Delete completed/cancelled jobs older than N days.
    /// Delete completed/cancelled jobs older than N days. Also sweeps
    /// orphan child jobs whose parent was deleted by this call.
    pub async fn delete_completed(db: &DatabaseConnection, older_than_days: i64) -> Result<u64, AppError> {
        let cutoff = Utc::now().fixed_offset() - chrono::Duration::days(older_than_days);
        let result = jobs::Entity::delete_many()
            .filter(jobs::Column::Status.is_in(["completed", "cancelled"]))
            .filter(jobs::Column::CompletedAt.lt(cutoff))
            .exec(db)
            .await?;
        let orphans = Self::cleanup_orphan_children(db).await?;
        Ok(result.rows_affected + orphans)
    }

    /// Delete all finished (completed / cancelled / failed) jobs for a given appId.
    /// Called before starting a new sync to ensure progress counts are accurate.
    pub async fn delete_finished_jobs_by_app_id(db: &DatabaseConnection, app_id: Uuid) -> Result<u64, AppError> {
        // Only delete top-level parents (parent_job_id IS NULL). Child
        // jobs follow their parent's lifecycle and are cleaned up via cascade
        // (cleanup_orphan_children) when the parent is removed. Deleting them
        // here would wipe child rows belonging to `partially_completed`
        // parents, breaking tree-view drill-down.
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"DELETE FROM jobs
               WHERE status IN ('completed', 'cancelled', 'failed')
                 AND payload->>'appId' = $1
                 AND parent_job_id IS NULL",
            [app_id.to_string().into()],
        );
        let result = db.execute_raw(stmt).await?;
        let orphans = Self::cleanup_orphan_children(db).await?;
        Ok(result.rows_affected() + orphans)
    }

    /// Bind alias rows to a leader by setting `alias_job_id = leader_id` and
    /// promoting them to `running`. Used by the scheduler to mark all
    /// pending aliases of an in-flight group as following the leader,
    /// without dispatching them to a worker.
    ///
    /// Returns the number of rows promoted.
    pub async fn bind_aliases_to_leader(
        db: &DatabaseConnection,
        leader_id: Uuid,
        alias_ids: &[Uuid],
    ) -> Result<u64, AppError> {
        if alias_ids.is_empty() {
            return Ok(0);
        }
        let result = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("running"))
            .col_expr(jobs::Column::AliasJobId, Expr::value(leader_id))
            .col_expr(jobs::Column::StartedAt, Expr::cust("NOW()"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.is_in(alias_ids.iter().copied()))
            .filter(jobs::Column::Status.eq("pending"))
            .exec(db)
            .await?;
        Ok(result.rows_affected)
    }

    /// Propagate a status/progress/error/meta update from a leader to all of its
    /// active alias rows. Skips members that have detached themselves
    /// (`status IN ('cancelled','suspended')`).
    pub async fn propagate_to_aliases(
        db: &DatabaseConnection,
        leader_id: Uuid,
        status: Option<&str>,
        progress: Option<i32>,
        error: Option<Option<&str>>,
        meta: Option<JsonValue>,
        completed: bool,
    ) -> Result<u64, AppError> {
        let mut q = jobs::Entity::update_many()
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::AliasJobId.eq(leader_id))
            .filter(jobs::Column::Status.is_not_in(["cancelled", "suspended"]));

        if let Some(s) = status {
            q = q.col_expr(jobs::Column::Status, Expr::value(s));
        }
        if let Some(p) = progress {
            q = q.col_expr(jobs::Column::Progress, Expr::value(p));
        }
        if let Some(e) = error {
            q = q.col_expr(
                jobs::Column::Error,
                match e {
                    Some(s) => Expr::value(s),
                    None => Expr::cust("NULL"),
                },
            );
        }
        if let Some(m) = meta {
            q = q.col_expr(jobs::Column::Meta, Expr::value(m));
        }
        if completed {
            q = q.col_expr(jobs::Column::CompletedAt, Expr::cust("NOW()"));
        }

        let result = q.exec(db).await?;
        Ok(result.rows_affected)
    }

    /// Find pending jobs for the worker to process.
    ///
    /// Ordered by `priority DESC, created_at ASC` so high-priority work
    /// (e.g. user-triggered single-photo jobs) overtakes background scans.
    /// Aliases are **not** filtered out: an alias's priority represents the
    /// group's current highest demand, and the scheduler dedupes by
    /// `coalesce(alias_job_id, id)` in Rust before dispatching.
    pub async fn find_pending_jobs(db: &DatabaseConnection, limit: u64) -> Result<Vec<jobs::Model>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"SELECT j.id, j.type, j.status, j.payload, j.meta,
                      j.progress, j.retry_count, j.max_retries, j.error,
                      j.started_at, j.completed_at, j.created_at, j.updated_at,
                      j.dedupe_key, j.alias_job_id, j.priority
               FROM jobs j
               WHERE j.status = 'pending'
               ORDER BY j.priority DESC, j.created_at ASC
               LIMIT $1",
            [limit.into()],
        );
        let models = jobs::Entity::find().from_raw_sql(stmt).all(db).await?;
        Ok(models)
    }

    /// Update job progress and meta.
    pub async fn update_progress(
        db: &DatabaseConnection,
        id: Uuid,
        progress: i32,
        meta: Option<JsonValue>,
    ) -> Result<Option<jobs::Model>, AppError> {
        let mut update = jobs::Entity::update_many()
            .col_expr(jobs::Column::Progress, Expr::value(progress))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id));

        if let Some(m) = meta {
            update = update.col_expr(jobs::Column::Meta, Expr::value(m));
        }

        let result = update.exec(db).await?;
        if result.rows_affected == 0 {
            return Ok(None);
        }
        let _ = Self::propagate_to_aliases(
            db,
            id,
            None,
            Some(progress),
            None,
            // Re-read meta from the just-updated leader so aliases get the
            // canonical persisted JSON (matches what update_progress wrote).
            jobs::Entity::find_by_id(id).one(db).await?.and_then(|m| m.meta),
            false,
        )
        .await?;
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    /// Mark job as running.
    pub async fn mark_running(db: &DatabaseConnection, id: Uuid) -> Result<Option<jobs::Model>, AppError> {
        let result = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("running"))
            .col_expr(jobs::Column::StartedAt, Expr::cust("NOW()"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            .filter(jobs::Column::Status.eq("pending"))  // atomic claim guard
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            return Ok(None);
        }
        let _ = Self::propagate_to_aliases(db, id, Some("running"), None, None, None, false).await?;
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    /// Mark job as completed.
    pub async fn mark_completed(
        db: &DatabaseConnection,
        id: Uuid,
        meta: Option<JsonValue>,
    ) -> Result<Option<jobs::Model>, AppError> {
        let mut update = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("completed"))
            .col_expr(jobs::Column::Progress, Expr::value(100))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NOW()"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            // Defensive guard: never overwrite a row that was already
            // flipped to a cancel/fail/suspend terminal by another code
            // path (e.g. `cancel_children_of` during a parent cancel race).
            .filter(jobs::Column::Status.is_not_in(["cancelled", "failed", "suspended"]));

        if let Some(m) = meta.clone() {
            update = update.col_expr(jobs::Column::Meta, Expr::value(m));
        }

        let result = update.exec(db).await?;
        if result.rows_affected == 0 {
            return Ok(None);
        }
        let _ = Self::propagate_to_aliases(db, id, Some("completed"), Some(100), Some(None), meta, true).await?;
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    /// Mark job as failed (also atomically increments `retry_count`).
    pub async fn mark_failed(db: &DatabaseConnection, id: Uuid, error: &str) -> Result<Option<jobs::Model>, AppError> {
        let result = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("failed"))
            .col_expr(jobs::Column::Error, Expr::value(error))
            .col_expr(jobs::Column::RetryCount, Expr::col(jobs::Column::RetryCount).add(1))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NOW()"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            return Ok(None);
        }
        let _ = Self::propagate_to_aliases(db, id, Some("failed"), None, Some(Some(error)), None, true).await?;
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    /// Mark job as `suspended`: observed a cooperative-cancel with
    /// `CancelReason::Suspended`. Clears any completion timestamp so the job
    /// can later be resumed back to `pending`.
    pub async fn mark_suspended(db: &DatabaseConnection, id: Uuid) -> Result<Option<jobs::Model>, AppError> {
        let result = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("suspended"))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            return Ok(None);
        }
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    /// Mark job as `cancelled`: observed a cooperative-cancel with
    /// `CancelReason::Aborted`. Stores the sentinel marker in `error` for
    /// diagnostics.
    pub async fn mark_cancelled(
        db: &DatabaseConnection,
        id: Uuid,
        error: &str,
    ) -> Result<Option<jobs::Model>, AppError> {
        let result = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("cancelled"))
            .col_expr(jobs::Column::Error, Expr::value(error))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NOW()"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            return Ok(None);
        }
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    /// Mark a parent job as `waiting`: it has finished its enqueue phase
    /// and is now awaiting child jobs to complete. Releases the worker slot
    /// without setting `completed_at`. Stores the supplied meta (e.g.
    /// `totalChildren`, `libraryName`).
    pub async fn mark_waiting(
        db: &DatabaseConnection,
        id: Uuid,
        meta: Option<JsonValue>,
    ) -> Result<Option<jobs::Model>, AppError> {
        let mut update = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("waiting"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            .filter(jobs::Column::Status.is_not_in(["cancelled", "failed", "suspended"]));

        if let Some(m) = meta {
            update = update.col_expr(jobs::Column::Meta, Expr::value(m));
        }

        let result = update.exec(db).await?;
        if result.rows_affected == 0 {
            return Ok(None);
        }
        Ok(jobs::Entity::find_by_id(id).one(db).await?)
    }

    /// Aggregate child-job results into the parent's `progress` and `meta`
    /// (`done`, `successes`, `failures`). Transitions `status` from
    /// `waiting` → `completed` when all children are done.
    ///
    /// Idempotent: safe to call from each child handler upon completion.
    ///
    /// `pending_success` / `pending_failure` let callers inject a bias for
    /// a child that hasn't yet been marked `completed`/`failed` in the DB
    /// (common case: called from the child handler's `finalize_child` path,
    /// which runs *before* the worker issues `mark_completed`). Pass `(0, 0)`
    /// when no bias is needed.
    pub async fn aggregate_parent_progress(
        db: &DatabaseConnection,
        parent_id: Uuid,
        pending_success: i32,
        pending_failure: i32,
    ) -> Result<Option<ParentAggregate>, AppError> {
        let sql = r"
WITH agg AS (
  SELECT
    COUNT(*) FILTER (WHERE status = 'completed')::int + $2::int AS s,
    COUNT(*) FILTER (WHERE status = 'failed')::int + $3::int AS f
  FROM jobs WHERE parent_job_id = $1::uuid
)
UPDATE jobs SET
  progress = LEAST(100, ((agg.s + agg.f) * 100 / GREATEST(1, (meta->>'totalChildren')::int))),
  meta = jsonb_set(jsonb_set(jsonb_set(meta,
            '{done}', to_jsonb(agg.s + agg.f)),
            '{successes}', to_jsonb(agg.s)),
            '{failures}', to_jsonb(agg.f)),
  status = CASE
    WHEN (agg.s + agg.f) >= (meta->>'totalChildren')::int THEN
      CASE
        WHEN agg.f = 0 THEN 'completed'
        WHEN agg.s = 0 THEN 'failed'
        ELSE 'partially_completed'
      END
    ELSE status
  END,
  completed_at = CASE WHEN (agg.s + agg.f) >= (meta->>'totalChildren')::int THEN NOW() ELSE completed_at END,
  updated_at = NOW()
FROM agg
WHERE id = $1::uuid AND meta ? 'totalChildren'
RETURNING
  (meta->>'totalChildren')::int AS total,
  (agg.s + agg.f) AS done,
  agg.s AS successes,
  agg.f AS failures,
  progress,
  status
";
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [
                parent_id.to_string().into(),
                pending_success.into(),
                pending_failure.into(),
            ],
        );
        let Some(row) = db.query_one_raw(stmt).await? else {
            return Ok(None);
        };
        let total: i32 = row.try_get_by_index(0)?;
        let done: i32 = row.try_get_by_index(1)?;
        let successes: i32 = row.try_get_by_index(2)?;
        let failures: i32 = row.try_get_by_index(3)?;
        let progress: i32 = row.try_get_by_index(4)?;
        let status: String = row.try_get_by_index(5)?;
        Ok(Some(ParentAggregate {
            total_children: total,
            done,
            successes,
            failures,
            progress,
            completed: matches!(status.as_str(), "completed" | "partially_completed" | "failed"),
        }))
    }

    /// Count jobs by status for a given appId (from payload JSONB).
    /// Returns (total, completed, running, pending, failed).
    pub async fn count_jobs_by_app(
        db: &DatabaseConnection,
        app_id: Uuid,
        job_types: &[&str],
    ) -> Result<(i64, i64, i64, i64, i64), AppError> {
        let types_csv = job_types.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(",");
        let sql = format!(
            r"SELECT
                 COUNT(*) AS total,
                 COUNT(*) FILTER (WHERE status = 'completed') AS completed,
                 COUNT(*) FILTER (WHERE status = 'running') AS running,
                 COUNT(*) FILTER (WHERE status = 'pending') AS pending,
                 COUNT(*) FILTER (WHERE status = 'failed') AS failed
               FROM jobs
               WHERE payload->>'appId' = $1
                 AND type IN ({types_csv})"
        );
        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &sql, [app_id.to_string().into()]);
        let row = db.query_one_raw(stmt).await?;
        match row {
            Some(r) => {
                let total: i64 = r.try_get_by_index(0)?;
                let completed: i64 = r.try_get_by_index(1)?;
                let running: i64 = r.try_get_by_index(2)?;
                let pending: i64 = r.try_get_by_index(3)?;
                let failed: i64 = r.try_get_by_index(4)?;
                Ok((total, completed, running, pending, failed))
            }
            None => Ok((0, 0, 0, 0, 0)),
        }
    }

    /// Per-type job progress with photo-level data for running AI jobs.
    ///
    /// Returns one row per job type: status counts + the most recent running
    /// job's `meta` (which contains `{total, processed, success}` for AI jobs).
    pub async fn get_task_progress_by_app(
        db: &DatabaseConnection,
        app_id: Uuid,
        job_types: &[&str],
    ) -> Result<Vec<TaskProgressRow>, AppError> {
        let types_csv = job_types.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(",");
        let sql = format!(
            r"SELECT
                 j1.type,
                 COUNT(*) FILTER (WHERE j1.status = 'completed') AS completed,
                 COUNT(*) FILTER (WHERE j1.status = 'running')   AS running,
                 COUNT(*) FILTER (WHERE j1.status = 'pending')   AS pending,
                 COUNT(*) FILTER (WHERE j1.status = 'failed')    AS failed,
                 (SELECT j2.meta FROM jobs j2
                   WHERE j2.type = j1.type
                     AND j2.payload->>'appId' = $1
                     AND j2.status = 'running'
                   ORDER BY j2.created_at DESC LIMIT 1
                 ) AS running_meta
               FROM jobs j1
               WHERE j1.payload->>'appId' = $1
                 AND j1.type IN ({types_csv})
               GROUP BY j1.type"
        );
        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &sql, [app_id.to_string().into()]);
        let rows = db.query_all_raw(stmt).await?;
        let mut result = Vec::with_capacity(rows.len());
        for r in rows {
            let job_type: String = r.try_get_by_index(0)?;
            let completed: i64 = r.try_get_by_index(1)?;
            let running: i64 = r.try_get_by_index(2)?;
            let pending: i64 = r.try_get_by_index(3)?;
            let failed: i64 = r.try_get_by_index(4)?;
            let running_meta: Option<JsonValue> = r.try_get_by_index(5)?;
            result.push(TaskProgressRow {
                job_type,
                completed,
                running,
                pending,
                failed,
                running_meta,
            });
        }
        Ok(result)
    }

    // ──────────────────────────────────────────────────────────────────────
    // Task-queue revamp: tree views, filter-aware bulk ops, single suspend
    // ──────────────────────────────────────────────────────────────────────

    /// List top-level jobs (those with no `parent_job_id`).
    ///
    /// Same shape as [`Self::list_jobs`] — returns `JobOutput` with
    /// `child_count` populated so the UI can decide whether a row is
    /// expandable.
    pub async fn list_root_jobs(
        db: &DatabaseConnection,
        filter: &JobListFilter,
        page: &PageInput,
    ) -> Result<Page<JobOutput>, AppError> {
        let mut query = jobs::Entity::find().order_by_desc(jobs::Column::CreatedAt);
        query = filter.apply(query, true);
        // Only roots: no parent_job_id.
        query = query.filter(jobs::Column::ParentJobId.is_null());

        let total = query.clone().count(db).await? as i64;
        let models = query
            .paginate(db, page.page_size)
            .fetch_page(page.page.saturating_sub(1))
            .await?;

        let child_counts =
            Self::count_children_for_parents(db, &models.iter().map(|m| m.id.to_string()).collect::<Vec<_>>()).await?;

        let items = models
            .into_iter()
            .map(|m| {
                let id = m.id.to_string();
                let mut out = JobOutput::from(m);
                out.child_count = child_counts.get(&id).copied().unwrap_or(0);
                out
            })
            .collect();
        Ok(Page::new(items, total, page))
    }

    /// List direct children of a given parent job (matches `parent_job_id =
    /// <parent_id>`). `child_count` is populated so nested trees can keep
    /// expanding.
    pub async fn list_children_by_parent(
        db: &DatabaseConnection,
        parent_id: Uuid,
        page: &PageInput,
    ) -> Result<Page<JobOutput>, AppError> {
        let query = jobs::Entity::find()
            .order_by_desc(jobs::Column::CreatedAt)
            .filter(jobs::Column::ParentJobId.eq(parent_id));

        let total = query.clone().count(db).await? as i64;
        let models = query
            .paginate(db, page.page_size)
            .fetch_page(page.page.saturating_sub(1))
            .await?;

        let child_counts =
            Self::count_children_for_parents(db, &models.iter().map(|m| m.id.to_string()).collect::<Vec<_>>()).await?;

        let items = models
            .into_iter()
            .map(|m| {
                let id = m.id.to_string();
                let mut out = JobOutput::from(m);
                out.child_count = child_counts.get(&id).copied().unwrap_or(0);
                out
            })
            .collect();
        Ok(Page::new(items, total, page))
    }

    /// Return the ids of all jobs matching the filter whose status is in
    /// `statuses`. Used by handlers to signal in-flight cancellation via
    /// [`crate::queue::CancellationRegistry`] *before* the bulk DB update
    /// flips the rows.
    pub async fn list_ids_by_filter_and_status(
        db: &DatabaseConnection,
        filter: &JobListFilter,
        statuses: &[&str],
    ) -> Result<Vec<Uuid>, AppError> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = jobs::Entity::find()
            .select_only()
            .column(jobs::Column::Id)
            .filter(jobs::Column::Status.is_in(statuses.iter().copied()));
        query = filter.apply(query, false);
        let ids: Vec<Uuid> = query.into_tuple().all(db).await?;
        Ok(ids)
    }

    /// Bulk-suspend all `pending` / `running` jobs matching the filter.
    ///
    /// The filter's `status` field is ignored — the target status set is
    /// fixed. Clears `completed_at` so jobs can later resume. Returns the
    /// number of rows affected.
    pub async fn suspend_pending_by_filter(db: &DatabaseConnection, filter: &JobListFilter) -> Result<u64, AppError> {
        let update = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("suspended"))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Status.is_in(["pending", "running"]));
        let update = filter.apply(update, false);
        let result = update.exec(db).await?;
        Ok(result.rows_affected)
    }

    /// Bulk-resume all `suspended` jobs matching the filter: clears
    /// completion timestamps and moves status back to `pending`.
    pub async fn resume_suspended_by_filter(db: &DatabaseConnection, filter: &JobListFilter) -> Result<u64, AppError> {
        let update = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("pending"))
            .col_expr(jobs::Column::Error, Expr::value::<Option<String>>(None))
            .col_expr(jobs::Column::StartedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Status.eq("suspended"));
        let update = filter.apply(update, false);
        let result = update.exec(db).await?;
        Ok(result.rows_affected)
    }

    /// Suspend a single job by id, but only if it is currently in an
    /// active state (`pending`, `running`, or `waiting`). Parent scan jobs
    /// that have finished enqueueing children sit in `waiting` until all
    /// children terminate, so suspending them must cover that case too.
    /// Returns `true` when a row was actually transitioned.
    pub async fn suspend_single(db: &DatabaseConnection, id: Uuid) -> Result<bool, AppError> {
        let result = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("suspended"))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            .filter(jobs::Column::Status.is_in(["pending", "running", "waiting"]))
            .exec(db)
            .await?;
        Ok(result.rows_affected > 0)
    }

    /// Resume a single suspended job back to `pending`. Returns `true` when
    /// a row was actually transitioned.
    pub async fn resume_single(db: &DatabaseConnection, id: Uuid) -> Result<bool, AppError> {
        let result = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("pending"))
            .col_expr(jobs::Column::Error, Expr::value::<Option<String>>(None))
            .col_expr(jobs::Column::StartedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NULL"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Id.eq(id))
            .filter(jobs::Column::Status.eq("suspended"))
            .exec(db)
            .await?;
        Ok(result.rows_affected > 0)
    }

    /// Bulk-mark `pending` + `running` jobs matching the filter as
    /// `cancelled`. The actual in-flight interruption for running jobs is
    /// fired from the handler via `CancellationRegistry`; this only writes
    /// the DB rows. Stamps `error` with [`CANCEL_MARKER_ABORTED`] for
    /// diagnostics.
    pub async fn abort_all_running_by_filter(db: &DatabaseConnection, filter: &JobListFilter) -> Result<u64, AppError> {
        let update = jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value("cancelled"))
            .col_expr(jobs::Column::Error, Expr::value(CANCEL_MARKER_ABORTED))
            .col_expr(jobs::Column::CompletedAt, Expr::cust("NOW()"))
            .col_expr(jobs::Column::UpdatedAt, Expr::cust("NOW()"))
            .filter(jobs::Column::Status.is_in(["pending", "running"]));
        let update = filter.apply(update, false);
        let result = update.exec(db).await?;
        Ok(result.rows_affected)
    }

    /// Prepare a job for being detached from its dedupe group (because the
    /// caller is about to cancel/suspend it):
    ///
    /// * If the job is an **alias** (`alias_job_id IS NOT NULL`): just clear
    ///   its `alias_job_id` so the leader's propagation no longer touches it.
    /// * If the job is a **leader** with active aliases: elect the earliest
    ///   active alias as the new leader (`alias_job_id = NULL`), re-point the
    ///   remaining active aliases to the new leader, and reset the new
    ///   leader's status back to `pending` so the scheduler picks it up
    ///   again. The original leader is left for the caller to flip to its
    ///   target terminal/suspended status.
    ///
    /// Atomic: runs in a single transaction so concurrent enqueues cannot
    /// observe an inconsistent group.
    pub async fn detach_and_elect(db: &DatabaseConnection, id: Uuid) -> Result<(), AppError> {
        let txn = db.begin().await?;
        let Some(j) = jobs::Entity::find_by_id(id).one(&txn).await? else {
            txn.commit().await?;
            return Ok(());
        };

        if j.alias_job_id.is_some() {
            // Self-detach: alias is leaving the group.
            let mut am: jobs::ActiveModel = j.into();
            am.alias_job_id = Set(None);
            am.update(&txn).await?;
            txn.commit().await?;
            return Ok(());
        }

        // j is a leader (or standalone). Find earliest active alias.
        let new_leader = jobs::Entity::find()
            .filter(jobs::Column::AliasJobId.eq(j.id))
            .filter(jobs::Column::Status.is_not_in(["cancelled", "suspended", "completed", "failed"]))
            .order_by_asc(jobs::Column::CreatedAt)
            .one(&txn)
            .await?;

        let Some(k) = new_leader else {
            // No active alias — group dissolves naturally.
            txn.commit().await?;
            return Ok(());
        };

        // Promote K to leader and reset to 'pending' so the scheduler
        // re-dispatches it (work needs to start from scratch since the
        // original leader is being cancelled/suspended mid-flight).
        let k_id = k.id;
        let mut k_am: jobs::ActiveModel = k.into();
        k_am.alias_job_id = Set(None);
        k_am.status = Set("pending".to_string());
        k_am.started_at = Set(None);
        k_am.completed_at = Set(None);
        k_am.progress = Set(0);
        k_am.update(&txn).await?;

        // Re-point remaining active aliases to the new leader.
        jobs::Entity::update_many()
            .col_expr(jobs::Column::AliasJobId, Expr::value(k_id))
            .filter(jobs::Column::AliasJobId.eq(j.id))
            .filter(jobs::Column::Id.ne(k_id))
            .filter(jobs::Column::Status.is_not_in(["cancelled", "suspended", "completed", "failed"]))
            .exec(&txn)
            .await?;

        txn.commit().await?;
        Ok(())
    }

    /// Resume a previously cancelled/suspended job:
    ///
    /// * If a different active leader exists for the same `(type, dedupe_key)`,
    ///   J rejoins the group as an alias (status mirrors the leader).
    /// * Otherwise J becomes its own leader and re-enters the `pending` set.
    ///
    /// Replaces the simple `resume_single` flow when the job has a
    /// `dedupe_key`; when there is no `dedupe_key` the behaviour is
    /// identical to the legacy reset-to-pending path.
    pub async fn resume_with_rejoin(db: &DatabaseConnection, id: Uuid) -> Result<bool, AppError> {
        let txn = db.begin().await?;
        let Some(j) = jobs::Entity::find_by_id(id).one(&txn).await? else {
            txn.commit().await?;
            return Ok(false);
        };

        // Only resume from terminal-ish states (suspended / cancelled / failed).
        if !matches!(j.status.as_str(), "suspended" | "cancelled" | "failed") {
            txn.commit().await?;
            return Ok(false);
        }

        let leader = if let Some(key) = j.dedupe_key.as_deref() {
            Self::find_active_by_dedupe(&txn, &j.r#type, key).await?
        } else {
            None
        };

        let mut am: jobs::ActiveModel = j.clone().into();
        am.error = Set(None);
        am.completed_at = Set(None);
        am.updated_at = Set(Utc::now().fixed_offset());

        match leader {
            Some(t) if t.id != j.id => {
                // Rejoin as alias — mirror leader's current state.
                let mirrored = match t.status.as_str() {
                    "running" | "waiting" => "running",
                    "pending" => "pending",
                    other => other,
                };
                am.alias_job_id = Set(Some(t.id));
                am.status = Set(mirrored.to_string());
                am.progress = Set(t.progress);
                am.started_at = Set(t.started_at);
            }
            _ => {
                // Stand alone: reset to pending leader.
                am.alias_job_id = Set(None);
                am.status = Set("pending".to_string());
                am.progress = Set(0);
                am.started_at = Set(None);
            }
        }

        am.update(&txn).await?;
        txn.commit().await?;
        Ok(true)
    }

    /// Filter-aware "soft" cleanup: delete every `completed` / `cancelled`
    /// job matching the filter. Failed / suspended jobs are kept so users
    /// can review or retry them. Age is ignored by design.
    pub async fn cleanup_all_finished_by_filter(
        db: &DatabaseConnection,
        filter: &JobListFilter,
    ) -> Result<u64, AppError> {
        let delete = jobs::Entity::delete_many().filter(jobs::Column::Status.is_in(["completed", "cancelled"]));
        let delete = filter.apply(delete, true);
        let result = delete.exec(db).await?;
        let orphans = Self::cleanup_orphan_children(db).await?;
        Ok(result.rows_affected + orphans)
    }

    /// Filter-aware "hard" purge: delete every job matching the filter
    /// regardless of status (including `pending` / `running` /
    /// `suspended` / `failed`). This is destructive and cannot be
    /// undone; callers must confirm with the user.
    pub async fn purge_all_by_filter(db: &DatabaseConnection, filter: &JobListFilter) -> Result<u64, AppError> {
        let delete = jobs::Entity::delete_many();
        let delete = filter.apply(delete, true);
        let result = delete.exec(db).await?;
        let orphans = Self::cleanup_orphan_children(db).await?;
        Ok(result.rows_affected + orphans)
    }
}

/// Raw per-type job data returned by [`JobRepo::get_task_progress_by_app`].
pub struct TaskProgressRow {
    pub job_type: String,
    pub completed: i64,
    pub running: i64,
    pub pending: i64,
    pub failed: i64,
    /// `meta` JSON from the most recent running job (if any).
    pub running_meta: Option<JsonValue>,
}
