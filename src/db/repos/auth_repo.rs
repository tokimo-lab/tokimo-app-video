use sea_orm::DatabaseConnection;
use crate::error::AppError;

pub struct AuthRepo;

impl AuthRepo {
    pub async fn get_user_id_by_session(
        _db: &DatabaseConnection,
        _session_id: &str,
    ) -> Result<Option<String>, AppError> {
        Ok(None)
    }

    pub async fn validate_internal_stream_token(
        _db: &DatabaseConnection,
        _token: &str,
    ) -> bool {
        false
    }
}
