//! `SeaORM` Entity for system_config table.

//! Shared public table; video sidecar must access it via bus/main-server APIs and must not write it directly.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "video", table_name = "system_config")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub scope: String,
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub scope_id: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub value: Json,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
