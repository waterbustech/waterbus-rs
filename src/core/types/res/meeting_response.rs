use chrono::NaiveDateTime;
use serde::Serialize;

use crate::core::entities::models::{Member, Message, Participant, User};

#[derive(Debug, Serialize)]
pub struct MeetingResponse {
    pub id: i32,
    pub title: String,
    pub avatar: Option<String>,
    pub status: i32,
    #[serde(skip_serializing)]
    pub password: String,
    #[serde(rename = "latestMessageCreatedAt")]
    pub latest_message_created_at: Option<NaiveDateTime>,
    pub code: i32,
    #[serde(rename = "createdAt")]
    pub created_at: NaiveDateTime,
    #[serde(rename = "updatedAt")]
    pub updated_at: NaiveDateTime,
    #[serde(rename = "deletedAt")]
    pub deleted_at: Option<NaiveDateTime>,
    pub members: Vec<Member>,
    pub participants: Vec<Participant>,
    pub latest_message: Option<Message>,
    pub created_by: Option<User>,
}
