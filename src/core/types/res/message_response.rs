use chrono::NaiveDateTime;
use serde::Serialize;

use crate::core::entities::models::{Meeting, User};

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: i32,
    pub data: String,
    pub type_: i32,
    pub status: i32,
    #[serde(rename = "createdAt")]
    pub created_at: NaiveDateTime,
    #[serde(rename = "updatedAt")]
    pub updated_at: NaiveDateTime,
    #[serde(rename = "deletedAt")]
    pub deleted_at: Option<NaiveDateTime>,
    #[serde(rename = "createdBy")]
    pub created_by: Option<User>,
    pub meeting: Option<Meeting>,
}
