use serde::Serialize;

use crate::core::entities::models::{Meeting, Member, Message, Participant, User};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingResponse {
    #[serde(flatten)]
    pub meeting: Meeting,
    pub members: Vec<MemberResponse>,
    pub participants: Vec<ParticipantResponse>,
    pub latest_message: Option<Message>,
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
