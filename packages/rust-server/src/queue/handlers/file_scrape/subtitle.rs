//! Subtitle file sync: discover external subtitle files and create DB records.

use bytes::Bytes;
use sea_orm::*;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

use crate::db::entities::subtitles;
use crate::services::storage::UploadOptions;
use crate::AppState;

use super::constants::{subtitle_ext_to_format, SUBTITLE_EXTENSIONS};
use super::parse::detect_subtitle_language;
use super::DirContext;

/// Sync external subtitle files from the directory to the subtitles table.
pub async fn sync_subtitles(
    db: &DatabaseConnection,
    state: &Arc<AppState>,
    file_id: Uuid,
    ctx: &DirContext,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let existing: Vec<subtitles::Model> = subtitles::Entity::find()
        .filter(subtitles::Column::FileId.eq(file_id))
        .all(db)
        .await?;
    let existing_paths: std::collections::HashSet<String> =
        existing.iter().filter_map(|s| s.path.clone()).collect();

    for entry in &ctx.dir_entries {
        let ext_lower = entry
            .rsplit('.')
            .next()
            .map(|e| format!(".{}", e.to_ascii_lowercase()))
            .unwrap_or_default();

        if !SUBTITLE_EXTENSIONS.contains(&ext_lower.as_str()) {
            continue;
        }

        // Subtitle filename must start with the video stem (case-sensitive, aligned with TS)
        if !entry.starts_with(&ctx.stem)
        {
            continue;
        }

        let sub_path = format!("{}/{}", ctx.dir_path.trim_end_matches('/'), entry);
        if existing_paths.contains(&sub_path) {
            continue;
        }

        let lang = detect_subtitle_language(entry);
        let format = subtitle_ext_to_format(&ext_lower);

        // Read and upload subtitle file
        let mut s3_key: Option<String> = None;
        if let Ok(buf) = ctx
            .vfs
            .read_bytes(std::path::Path::new(&sub_path), 0, None)
            .await
        {
            let uid = Uuid::new_v4();
            let key = format!("subtitles/{file_id}/{uid}.{format}");
            let content_type = if format == "vtt" {
                "text/vtt"
            } else {
                "text/plain; charset=utf-8"
            };
            match state
                .storage
                .upload(
                    &key,
                    Bytes::from(buf),
                    Some(UploadOptions {
                        content_type: Some(content_type.to_string()),
                    }),
                )
                .await
            {
                Ok(()) => s3_key = Some(key),
                Err(e) => warn!("[file_scrape] Failed to upload subtitle {sub_path}: {e}"),
            }
        }

        let model = subtitles::ActiveModel {
            id: Set(Uuid::new_v4()),
            file_id: Set(file_id),
            language: Set(lang),
            title: Set(None),
            source_type: Set("external".to_string()),
            format: Set(format.to_string()),
            path: Set(Some(sub_path)),
            s3_key: Set(s3_key),
            source: Set(None),
            source_id: Set(None),
            encoding: Set(None),
            is_default: Set(false),
            is_forced: Set(false),
            created_at: Set(chrono::Utc::now().fixed_offset()),
            is_hearing_impaired: Set(false),
        };

        if let Err(e) = subtitles::Entity::insert(model).exec(db).await {
            warn!("[file_scrape] Failed to insert subtitle record: {e}");
        }
    }

    Ok(())
}
