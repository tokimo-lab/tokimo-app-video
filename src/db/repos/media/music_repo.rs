// =============================================================================
// ⚠️ CROSS-APP DEPRECATED: music repo inside video sidecar ⚠️
// =============================================================================
// Music domain code does not belong in the video app. Lives here only because
// the video sidecar inherited shared media repositories during extraction.
//
// DEADLINE: remove once `tokimo-app-music` sidecar exists and owns these tables.
// Until then, treat this as read-only legacy — DO NOT add features here.
// See plan.md F9 (cross-app marker).
//
// COMMENTED OUT IN B4: music entities removed from video app
// =============================================================================

/*
// Original content commented out - entities no longer available
// Re-enable this when music entities are restored or moved to proper app

use chrono::Utc;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{sea_query::Expr, *};
use uuid::Uuid;

use crate::db::entities::{music_files, musics, vfs};
use crate::db::models::media::file::MediaFileStreamTarget;
use crate::error::AppError;
use crate::error::OptionExt;

... rest of file content ...
*/
