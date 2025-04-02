use chrono::NaiveDateTime;
use serde::Serialize;

use crate::core::entities::models::{MeetingsStatusEnum, Member, Message, Participant, User};

#[derive(Debug, Serialize)]
pub struct MeetingResponse {
    pub id: i32,
    pub title: String,
    pub avatar: Option<String>,
    pub status: MeetingsStatusEnum,
    #[serde(rename = "latestMessageCreatedAt")]
    pub latest_message_created_at: Option<NaiveDateTime>,
    pub code: i32,
    #[serde(rename = "createdAt")]
    pub created_at: NaiveDateTime,
    #[serde(rename = "updatedAt")]
    pub updated_at: NaiveDateTime,
    #[serde(rename = "deletedAt")]
    pub deleted_at: Option<NaiveDateTime>,
    pub member: Option<Member>,
    pub participant: Option<Participant>,
    pub latest_message: Option<Message>,
    pub created_by: Option<User>,
}
