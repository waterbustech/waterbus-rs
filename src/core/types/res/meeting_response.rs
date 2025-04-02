use serde::Serialize;

use crate::core::entities::models::{Meeting, Member, Message, Participant, User};

#[derive(Debug, Serialize)]
pub struct MeetingResponse {
    pub meeting: Meeting,
    pub member: Option<Member>,
    pub participant: Option<Participant>,
    pub latest_message: Option<Message>, // or whatever fields you need from Message
    pub created_by: Option<User>,        // Assuming you want user info for the message
}
