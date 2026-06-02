use crate::db::ApiDateTimeExt;
use sea_orm::*;
use tokimo_package_ffmpeg::normalize_subtitle_codec;
use uuid::Uuid;

use crate::db::entities::{subtitles, video_files};
use crate::db::models::subtitle::{FileSubtitleRow, SubtitleRecord};
use crate::error::AppError;

/// Input for creating a subtitle record after downloading from an external provider.
#[derive(Debug)]
pub struct CreateSubtitleInput {
    pub file_id: String,
    pub language: String,
    pub title: Option<String>,
    pub format: String,
    pub source: String,
    pub source_id: Option<String>,
    pub s3_key: String,
}

pub struct SubtitleRepo;

impl SubtitleRepo {
    /// Load all embedded subtitles for a media file, together with ffprobe data.
    pub async fn load_file_subtitles<C: ConnectionTrait>(db: &C, file_id: &str) -> Result<Vec<FileSubtitleRow>, AppError> {
        let fid: Uuid = file_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid file id".into()))?;

        // Load subtitles with embedded source_type
        let sub_rows = subtitles::Entity::find()
            .filter(subtitles::Column::FileId.eq(fid))
            .filter(subtitles::Column::SourceType.eq("embedded"))
            .order_by_desc(subtitles::Column::IsDefault)
            .order_by_asc(subtitles::Column::Language)
            .order_by_asc(subtitles::Column::CreatedAt)
            .all(db)
            .await?;

        // Load ffprobe_raw from the media_file
        let ffprobe_raw = video_files::Entity::find_by_id(fid)
            .one(db)
            .await?
            .and_then(|vf| vf.ffprobe_raw);

        Ok(sub_rows
            .into_iter()
            .map(|s| FileSubtitleRow {
                id: s.id.to_string(),
                language: s.language,
                title: s.title,
                format: normalize_subtitle_codec(&s.format),
                is_default: s.is_default,
                is_forced: s.is_forced,
                source_id: s.source_id,
                ffprobe_raw: ffprobe_raw.clone(),
            })
            .collect())
    }

    /// Load ALL subtitles for a media file (embedded + downloaded + external).
    pub async fn get_all_file_subtitles<C: ConnectionTrait>(
        db: &C,
        file_id: &str,
        storage_base_url: &str,
    ) -> Result<Vec<SubtitleRecord>, AppError> {
        let fid: Uuid = file_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid file id".into()))?;

        let rows = subtitles::Entity::find()
            .filter(subtitles::Column::FileId.eq(fid))
            .order_by_desc(subtitles::Column::IsDefault)
            .order_by_asc(subtitles::Column::Language)
            .order_by_asc(subtitles::Column::CreatedAt)
            .all(db)
            .await?;

        let base = storage_base_url.trim_end_matches('/');
        Ok(rows
            .into_iter()
            .map(|s| {
                let stream_index = s.source_id.as_deref().and_then(|id| id.parse::<i32>().ok());
                let storage_url = s.s3_key.as_deref().map(|k| format!("{base}/{k}"));
                SubtitleRecord {
                    id: s.id.to_string(),
                    language: s.language,
                    title: s.title,
                    source_type: s.source_type,
                    format: normalize_subtitle_codec(&s.format),
                    is_default: s.is_default,
                    is_forced: s.is_forced,
                    is_hearing_impaired: s.is_hearing_impaired,
                    stream_index,
                    storage_url,
                    source: s.source,
                    created_at: s.created_at.to_api_datetime(),
                }
            })
            .collect())
    }

    /// Create a new subtitle record after downloading from an external provider.
    pub async fn create_subtitle<C: ConnectionTrait>(
        db: &C,
        input: CreateSubtitleInput,
    ) -> Result<subtitles::Model, AppError> {
        use sea_orm::ActiveValue::Set;

        let fid: Uuid = input
            .file_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid file id".into()))?;

        let model = subtitles::ActiveModel {
            id: Set(Uuid::new_v4()),
            file_id: Set(fid),
            language: Set(input.language),
            title: Set(input.title),
            source_type: Set("downloaded".to_string()),
            format: Set(input.format),
            path: Set(None),
            s3_key: Set(Some(input.s3_key)),
            source: Set(Some(input.source)),
            source_id: Set(input.source_id),
            encoding: Set(None),
            is_default: Set(false),
            is_forced: Set(false),
            is_hearing_impaired: Set(false),
            created_at: Set(chrono::Utc::now().into()),
        };

        let inserted = subtitles::Entity::insert(model).exec_with_returning(db).await?;
        Ok(inserted)
    }

    /// Find a subtitle record by ID.
    pub async fn find_by_id<C: ConnectionTrait>(db: &C, subtitle_id: &str) -> Result<Option<subtitles::Model>, AppError> {
        let sid: Uuid = subtitle_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid subtitle id".into()))?;
        let row = subtitles::Entity::find_by_id(sid).one(db).await?;
        Ok(row)
    }

    /// Delete a subtitle record by ID. Returns the `s3_key` (if any) so the caller can clean up storage.
    pub async fn delete_subtitle<C: ConnectionTrait>(db: &C, subtitle_id: &str) -> Result<Option<String>, AppError> {
        let sid: Uuid = subtitle_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid subtitle id".into()))?;
        let row = subtitles::Entity::find_by_id(sid).one(db).await?;
        let s3_key = row.as_ref().and_then(|r| r.s3_key.clone());
        subtitles::Entity::delete_by_id(sid).exec(db).await?;
        Ok(s3_key)
    }
}
