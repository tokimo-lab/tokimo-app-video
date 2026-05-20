// =============================================================================
// ⚠️ CROSS-APP DEPRECATED: book repo inside video sidecar ⚠️
// =============================================================================
// Book domain code does not belong in the video app. Lives here only because the
// video sidecar inherited shared media repositories during extraction.
//
// DEADLINE: remove once `tokimo-app-book` sidecar exists and owns these tables.
// Until then, treat this as read-only legacy — DO NOT add features here.
// See plan.md F9 (cross-app marker).
//
// COMMENTED OUT IN B4: book entities removed from video app
// =============================================================================

/*
// Original content commented out - entities no longer available
// Re-enable this when book entities are restored or moved to proper app

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, EntityTrait, Order,
    QueryFilter, QueryOrder, Set, Statement, prelude::DateTimeWithTimeZone, sea_query::Expr,
};
use uuid::Uuid;

use crate::db::entities::{book_chapters, book_files, book_items, book_volumes, books};
use crate::error::AppError;
use crate::error::OptionExt;

... rest of file content ...
*/
