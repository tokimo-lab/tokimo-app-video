use sea_orm::*;
use uuid::Uuid;

use crate::db::entities::{vfs, video_files};
use crate::db::models::media::file::MediaFileStreamTarget;
use crate::error::AppError;

pub struct VideoFileRepo;

impl VideoFileRepo {
    /// Load the minimal info needed to stream a video file: its path, source
    /// type, and whether it belongs to a media server.
    pub async fn load_stream_target<C: ConnectionTrait>(
        db: &C,
        file_id: &str,
    ) -> Result<Option<MediaFileStreamTarget>, AppError> {
        let fid: Uuid = file_id
            .parse()
            .map_err(|_| AppError::BadRequest("invalid file id".into()))?;
        let row = video_files::Entity::find_by_id(fid)
            .find_also_related(vfs::Entity)
            .one(db)
            .await?;

        Ok(row.map(|(vf, fs)| {
            let (source_type, source_config) = match fs {
                Some(s) => (Some(s.r#type), s.config),
                None => (None, None),
            };
            MediaFileStreamTarget {
                path: vf.path,
                source_id: vf.source_id.map(|id| id.to_string()),
                source_type,
                source_config,
                video_item_id: vf.video_item_id.map(|id| id.to_string()),
                episode_id: vf.episode_id.map(|id| id.to_string()),
                duration: vf.duration.map(f64::from),
                size: vf.size,
            }
        }))
    }
}
