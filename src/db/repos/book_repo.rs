// =============================================================================
// ⚠️ CROSS-APP DEPRECATED: book repo inside video sidecar ⚠️
// =============================================================================
// Book domain code does not belong in the video app. Lives here only because the
// video sidecar inherited shared media repositories during extraction.
//
// DEADLINE: remove once `tokimo-app-book` sidecar exists and owns these tables.
// Until then, treat this as read-only legacy — DO NOT add features here.
// See plan.md F9 (cross-app marker).
// =============================================================================

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, EntityTrait, Order,
    QueryFilter, QueryOrder, Set, Statement, prelude::DateTimeWithTimeZone, sea_query::Expr,
};
use uuid::Uuid;

use crate::db::entities::{book_chapters, book_files, book_items, book_volumes, books};
use crate::error::AppError;
use crate::error::OptionExt;

// ── Item input structs ──────────────────────────────────────────────────────

pub struct CreateBookItemInput {
    pub id: Uuid,
    pub book_id: Uuid,
    pub title: String,
    pub author: Option<String>,
    pub overview: Option<String>,
    pub serial_status: Option<String>,
    pub word_count: Option<i32>,
    pub year: Option<i32>,
    pub source_provider: Option<String>,
    pub source_book_id: Option<String>,
}

pub struct InsertVolumeInput {
    pub id: Uuid,
    pub book_id: Uuid,
    pub volume_number: i32,
    pub title: Option<String>,
    pub chapter_count: Option<i32>,
}

pub struct InsertChapterInput {
    pub id: Uuid,
    pub book_id: Uuid,
    pub volume_id: Option<Uuid>,
    pub chapter_number: i32,
    pub title: Option<String>,
    pub word_count: Option<i32>,
    pub file_path: Option<String>,
    pub is_vip: bool,
}

// ── Container update fields ──

#[derive(Debug)]
pub struct UpdateBookContainerFields {
    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar: Option<serde_json::Value>,
    pub poster_path: Option<String>,
    pub scrape_enabled: Option<bool>,
    pub scrape_agents: Option<Vec<String>>,
    pub settings: Option<serde_json::Value>,
    pub sources: Option<serde_json::Value>,
}

// ── Helpers ──

fn col<T: sea_orm::TryGetable>(r: &sea_orm::QueryResult, c: &str) -> Result<T, AppError> {
    r.try_get::<T>("", c)
        .map_err(|e| AppError::Internal(format!("col '{c}': {e:?}")))
}

fn opt<T: sea_orm::TryGetable>(r: &sea_orm::QueryResult, c: &str) -> Option<T> {
    r.try_get::<Option<T>>("", c).ok().flatten()
}

fn dir(d: &str) -> &'static str {
    if d.eq_ignore_ascii_case("desc") { "DESC" } else { "ASC" }
}

pub struct BookRepo;

impl BookRepo {
    // ── Container (books table) methods ─────────────────────────────────

    pub async fn list_containers(db: &DatabaseConnection) -> Result<Vec<books::Model>, AppError> {
        let rows = books::Entity::find()
            .order_by_asc(books::Column::SortOrder)
            .order_by_asc(books::Column::CreatedAt)
            .all(db)
            .await?;
        Ok(rows)
    }

    pub async fn get_container_by_id(db: &DatabaseConnection, id: Uuid) -> Result<Option<books::Model>, AppError> {
        Ok(books::Entity::find_by_id(id).one(db).await?)
    }

    pub async fn create_container(
        db: &DatabaseConnection,
        name: String,
        book_type: String,
        settings: Option<serde_json::Value>,
    ) -> Result<books::Model, AppError> {
        let id = Uuid::new_v4();
        let now = Utc::now().fixed_offset();
        let max_sort = books::Entity::find()
            .order_by_desc(books::Column::SortOrder)
            .one(db)
            .await?
            .map_or(0, |m| m.sort_order);

        let active = books::ActiveModel {
            id: Set(id),
            name: Set(name),
            r#type: Set(book_type),
            sort_order: Set(max_sort + 1),
            settings: Set(settings),
            sources: Set(serde_json::json!([])),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        books::Entity::insert(active).exec(db).await?;
        books::Entity::find_by_id(id)
            .one(db)
            .await?
            .internal("failed to fetch created book container")
    }

    pub async fn update_container(
        db: &DatabaseConnection,
        id: Uuid,
        input: UpdateBookContainerFields,
    ) -> Result<books::Model, AppError> {
        let model = books::Entity::find_by_id(id)
            .one(db)
            .await?
            .not_found(format!("book {id} not found"))?;
        let mut active: books::ActiveModel = model.into();
        if let Some(name) = input.name {
            active.name = Set(name);
        }
        if let Some(description) = input.description {
            active.description = Set(Some(description));
        }
        if let Some(avatar) = input.avatar {
            active.avatar = Set(Some(avatar));
        }
        if let Some(poster_path) = input.poster_path {
            active.poster_path = Set(Some(poster_path));
        }
        if let Some(scrape_enabled) = input.scrape_enabled {
            active.scrape_enabled = Set(scrape_enabled);
        }
        if let Some(scrape_agents) = input.scrape_agents {
            active.scrape_agents = Set(Some(scrape_agents));
        }
        if let Some(settings) = input.settings {
            active.settings = Set(Some(settings));
        }
        if let Some(sources) = input.sources {
            active.sources = Set(sources);
        }
        active.updated_at = Set(Some(Utc::now().fixed_offset()));
        let updated = active.update(db).await?;
        Ok(updated)
    }

    pub async fn delete_container(db: &DatabaseConnection, id: Uuid) -> Result<u64, AppError> {
        let result = books::Entity::delete_by_id(id).exec(db).await?;
        Ok(result.rows_affected)
    }

    pub async fn reorder_containers(db: &DatabaseConnection, orders: Vec<(Uuid, i32)>) -> Result<(), AppError> {
        for (id, sort_order) in orders {
            books::Entity::update_many()
                .filter(books::Column::Id.eq(id))
                .col_expr(books::Column::SortOrder, Expr::value(sort_order))
                .exec(db)
                .await?;
        }
        Ok(())
    }

    pub async fn update_sync_status(
        db: &DatabaseConnection,
        id: Uuid,
        status: &str,
        last_sync_at: Option<DateTimeWithTimeZone>,
    ) -> Result<(), AppError> {
        let model = books::Entity::find_by_id(id)
            .one(db)
            .await?
            .not_found(format!("book {id} not found"))?;
        let mut active: books::ActiveModel = model.into();
        active.sync_status = Set(status.to_string());
        if let Some(ts) = last_sync_at {
            active.last_sync_at = Set(Some(ts));
        }
        active.updated_at = Set(Some(Utc::now().fixed_offset()));
        active.update(db).await?;
        Ok(())
    }

    /// Parse sources JSON from the container. Returns `(source_id, root_path, is_default_download)`.
    pub fn parse_sources(sources_json: &serde_json::Value) -> Vec<(Uuid, String, bool)> {
        sources_json
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let source_id = item
                            .get("sourceId")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse::<Uuid>().ok())?;
                        let root_path = item
                            .get("rootPath")
                            .and_then(|v| v.as_str())
                            .map(std::string::ToString::to_string)?;
                        let is_default = item
                            .get("isDefaultDownload")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        Some((source_id, root_path, is_default))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get the first source for a book container (source_id, root_path).
    pub async fn get_book_source(db: &DatabaseConnection, book_id: Uuid) -> Result<Option<(Uuid, String)>, AppError> {
        let container = books::Entity::find_by_id(book_id).one(db).await?;
        let Some(container) = container else {
            return Ok(None);
        };
        let sources = Self::parse_sources(&container.sources);
        Ok(sources.first().map(|(sid, rp, _)| (*sid, rp.clone())))
    }

    // ── Item (book_items table) methods ─────────────────────────────────

    /// Paginated book item list for a container, with chapter/volume counts.
    pub async fn list_items(
        db: &DatabaseConnection,
        book_id: Uuid,
        page: i64,
        page_size: i64,
        sort_by: &str,
        sort_dir: &str,
        search: Option<&str>,
    ) -> Result<(Vec<serde_json::Value>, i64), AppError> {
        let order_col = match sort_by {
            "year" => "n.year",
            "wordCount" => "n.word_count",
            "addedAt" | "createdAt" => "n.created_at",
            "author" => "n.author",
            _ => "n.title",
        };
        let order_dir = dir(sort_dir);

        let mut where_clauses = vec!["n.book_id = $1".to_string()];
        let mut params: Vec<sea_orm::Value> = vec![book_id.into()];
        let mut param_idx = 2u32;

        if let Some(s) = search
            && !s.is_empty()
        {
            where_clauses.push(format!("(n.title ILIKE ${param_idx} OR n.author ILIKE ${param_idx})"));
            params.push(format!("%{s}%").into());
            param_idx += 1;
        }

        let where_sql = where_clauses.join(" AND ");

        // Count
        let count_sql = format!("SELECT COUNT(*) as cnt FROM book_items n WHERE {where_sql}");
        let count_stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &count_sql, params.clone());
        let total: i64 = db
            .query_one_raw(count_stmt)
            .await?
            .map_or(0, |r| col::<i64>(&r, "cnt").unwrap_or(0));

        // Items
        let limit_param = param_idx;
        let offset_param = param_idx + 1;
        let items_sql = format!(
            r"SELECT n.id, n.title, n.author, n.overview, n.cover_path, n.serial_status,
                      n.word_count, n.year, n.source_provider, n.is_favorite,
                      n.scraped_at::text as scraped_at, n.created_at,
                      (SELECT COUNT(*) FROM book_chapters nc WHERE nc.book_id = n.id) as chapter_count,
                      (SELECT COUNT(*) FROM book_volumes nv WHERE nv.book_id = n.id) as volume_count
               FROM book_items n
               WHERE {where_sql}
               ORDER BY {order_col} {order_dir} NULLS LAST
               LIMIT ${limit_param} OFFSET ${offset_param}"
        );
        params.push(page_size.into());
        params.push(((page - 1) * page_size).into());

        let items_stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &items_sql, params);
        let rows = db.query_all_raw(items_stmt).await?;

        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": col::<Uuid>(r, "id").unwrap_or_default().to_string(),
                    "title": col::<String>(r, "title").unwrap_or_default(),
                    "author": opt::<String>(r, "author"),
                    "overview": opt::<String>(r, "overview"),
                    "coverPath": opt::<String>(r, "cover_path"),
                    "serialStatus": opt::<String>(r, "serial_status"),
                    "wordCount": opt::<i32>(r, "word_count"),
                    "year": opt::<i32>(r, "year"),
                    "sourceProvider": opt::<String>(r, "source_provider"),
                    "isFavorite": col::<bool>(r, "is_favorite").unwrap_or(false),
                    "chapterCount": col::<i64>(r, "chapter_count").unwrap_or(0),
                    "volumeCount": col::<i64>(r, "volume_count").unwrap_or(0),
                    "scrapedAt": opt::<String>(r, "scraped_at"),
                    "createdAt": opt::<chrono::DateTime<chrono::FixedOffset>>(r, "created_at")
                        .map(|d| d.to_rfc3339()),
                })
            })
            .collect();

        Ok((items, total))
    }

    /// Get a single book item by ID.
    pub async fn get_item_by_id(db: &DatabaseConnection, id: Uuid) -> Result<Option<book_items::Model>, AppError> {
        Ok(book_items::Entity::find_by_id(id).one(db).await?)
    }

    /// Get all volumes for a book item, ordered by `volume_number`.
    pub async fn get_volumes(db: &DatabaseConnection, book_id: Uuid) -> Result<Vec<book_volumes::Model>, AppError> {
        Ok(book_volumes::Entity::find()
            .filter(book_volumes::Column::BookId.eq(book_id))
            .order_by_asc(book_volumes::Column::VolumeNumber)
            .all(db)
            .await?)
    }

    /// Get all chapters for a book item, ordered by `chapter_number`.
    pub async fn get_chapters(db: &DatabaseConnection, book_id: Uuid) -> Result<Vec<book_chapters::Model>, AppError> {
        Ok(book_chapters::Entity::find()
            .filter(book_chapters::Column::BookId.eq(book_id))
            .order_by_asc(book_chapters::Column::ChapterNumber)
            .all(db)
            .await?)
    }

    /// Get a single chapter by ID.
    pub async fn get_chapter_by_id(
        db: &DatabaseConnection,
        id: Uuid,
    ) -> Result<Option<book_chapters::Model>, AppError> {
        Ok(book_chapters::Entity::find_by_id(id).one(db).await?)
    }

    /// Get the previous chapter (by `chapter_number`) within the same book item.
    pub async fn get_prev_chapter(
        db: &DatabaseConnection,
        book_id: Uuid,
        chapter_number: i32,
    ) -> Result<Option<book_chapters::Model>, AppError> {
        Ok(book_chapters::Entity::find()
            .filter(book_chapters::Column::BookId.eq(book_id))
            .filter(book_chapters::Column::ChapterNumber.lt(chapter_number))
            .order_by(book_chapters::Column::ChapterNumber, Order::Desc)
            .one(db)
            .await?)
    }

    /// Get the next chapter (by `chapter_number`) within the same book item.
    pub async fn get_next_chapter(
        db: &DatabaseConnection,
        book_id: Uuid,
        chapter_number: i32,
    ) -> Result<Option<book_chapters::Model>, AppError> {
        Ok(book_chapters::Entity::find()
            .filter(book_chapters::Column::BookId.eq(book_id))
            .filter(book_chapters::Column::ChapterNumber.gt(chapter_number))
            .order_by_asc(book_chapters::Column::ChapterNumber)
            .one(db)
            .await?)
    }

    /// Get files linked to a book item.
    pub async fn get_book_files(db: &DatabaseConnection, book_id: Uuid) -> Result<Vec<book_files::Model>, AppError> {
        Ok(book_files::Entity::find()
            .filter(book_files::Column::BookId.eq(book_id))
            .all(db)
            .await?)
    }

    /// Insert a new book item record.
    pub async fn create_item(db: &DatabaseConnection, input: CreateBookItemInput) -> Result<(), AppError> {
        let now = chrono::Utc::now().fixed_offset();
        let active = book_items::ActiveModel {
            id: Set(input.id),
            book_id: Set(input.book_id),
            title: Set(input.title),
            author: Set(input.author),
            overview: Set(input.overview),
            serial_status: Set(input.serial_status),
            word_count: Set(input.word_count),
            year: Set(input.year),
            source_provider: Set(input.source_provider),
            source_book_id: Set(input.source_book_id),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        book_items::Entity::insert(active).exec(db).await?;
        Ok(())
    }

    /// Update the cover image path for a book item.
    pub async fn update_cover_path(db: &DatabaseConnection, book_id: Uuid, path: String) -> Result<(), AppError> {
        let now = chrono::Utc::now().fixed_offset();
        let model = book_items::Entity::find_by_id(book_id)
            .one(db)
            .await?
            .not_found("Book not found")?;
        let mut active: book_items::ActiveModel = model.into();
        active.cover_path = Set(Some(path));
        active.updated_at = Set(Some(now));
        active.update(db).await?;
        Ok(())
    }

    /// Insert a book volume.
    pub async fn insert_volume(db: &DatabaseConnection, input: InsertVolumeInput) -> Result<(), AppError> {
        let now = chrono::Utc::now().fixed_offset();
        let active = book_volumes::ActiveModel {
            id: Set(input.id),
            book_id: Set(input.book_id),
            volume_number: Set(input.volume_number),
            title: Set(input.title),
            chapter_count: Set(input.chapter_count),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        book_volumes::Entity::insert(active).exec(db).await?;
        Ok(())
    }

    /// Insert a book chapter.
    pub async fn insert_chapter(db: &DatabaseConnection, input: InsertChapterInput) -> Result<(), AppError> {
        let now = chrono::Utc::now().fixed_offset();
        let active = book_chapters::ActiveModel {
            id: Set(input.id),
            book_id: Set(input.book_id),
            volume_id: Set(input.volume_id),
            chapter_number: Set(input.chapter_number),
            title: Set(input.title),
            word_count: Set(input.word_count),
            file_path: Set(input.file_path),
            is_vip: Set(input.is_vip),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        book_chapters::Entity::insert(active).exec(db).await?;
        Ok(())
    }

    /// Get a single book volume by ID.
    pub async fn get_volume_by_id(
        db: &DatabaseConnection,
        volume_id: Uuid,
    ) -> Result<Option<book_volumes::Model>, AppError> {
        Ok(book_volumes::Entity::find_by_id(volume_id).one(db).await?)
    }
}
