use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use crate::error::AppError;

pub struct SystemConfigRepo;

impl SystemConfigRepo {
    pub async fn get<T: Default + for<'de> Deserialize<'de>>(
        _db: &DatabaseConnection,
    ) -> Result<T, AppError> {
        Ok(T::default())
    }

    pub async fn set<T: Serialize>(
        _db: &DatabaseConnection,
        _value: &T,
    ) -> Result<(), AppError> {
        Ok(())
    }
}
