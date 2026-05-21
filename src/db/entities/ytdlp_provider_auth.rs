//! SeaORM entity for `ytdlp_provider_auth` (video schema).
//! Owned by app-video; mirrors prisma model `YtdlpProviderAuth`. Keep in sync.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "video", table_name = "ytdlp_provider_auth")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub provider: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub value: Json,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
