use chrono::NaiveDateTime;
use serde::Serialize;

use crate::core::entities::models::{Meeting, User};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageResponse {
    pub id: i32,
    pub data: String,
    pub type_: i32,
    pub status: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub created_by: Option<User>,
    pub meeting: Option<Meeting>,
}
