use serde::Serialize;

use crate::core::entities::models::{Meeting, Member, Participant, User};

use super::message_response::MessageResponse;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingResponse {
    #[serde(flatten)]
    pub meeting: Meeting,
    pub members: Vec<MemberResponse>,
    pub participants: Vec<ParticipantResponse>,
    pub latest_message: Option<MessageResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberResponse {
    #[serde(flatten)]
    pub member: Member,
    pub user: Option<User>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantResponse {
    #[serde(flatten)]
    pub participant: Participant,
    pub user: Option<User>,
}
