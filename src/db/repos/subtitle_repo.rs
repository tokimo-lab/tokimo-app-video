use sea_orm::DatabaseConnection;
use serde::Deserialize;
use crate::db::models::subtitle::{FileSubtitleRow, SubtitleRecord};
use crate::error::AppError;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSubtitleInput {
    pub file_id: String,
    pub language: String,
    pub title: Option<String>,
    pub source_type: String,
    pub format: String,
    pub file_path: String,
    pub is_default: bool,
    pub is_forced: bool,
}

pub struct SubtitleRepo;

impl SubtitleRepo {
    pub async fn load_file_subtitles(
        _db: &DatabaseConnection,
        _file_id: &str,
    ) -> Result<Vec<FileSubtitleRow>, AppError> {
        Ok(vec![])
    }

    pub async fn get_all_file_subtitles(
        _db: &DatabaseConnection,
        _file_id: &str,
        _base: &str,
    ) -> Result<Vec<SubtitleRecord>, AppError> {
        Ok(vec![])
    }

    pub async fn create_subtitle(
        _db: &DatabaseConnection,
        _input: CreateSubtitleInput,
    ) -> Result<SubtitleRecord, AppError> {
        Err(AppError::Internal("not implemented".into()))
    }

    pub async fn delete_subtitle(
        _db: &DatabaseConnection,
        _subtitle_id: &str,
    ) -> Result<(), AppError> {
        Ok(())
    }
}
