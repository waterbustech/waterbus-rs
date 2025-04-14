use serde::Serialize;
use webrtc_manager::models::{IceCandidate, SubscribeResponse};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]

pub struct ParticipantHasLeftResponse {
    pub target_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandleRaisingResponse {
    pub participant_id: String,
    pub is_raising: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSharingResponse {
    pub participant_id: String,
    pub is_sharing: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnabledResponse {
    pub participant_id: String,
    pub is_enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraTypeResponse {
    pub participant_id: String,
    #[serde(rename = "type")]
    pub type_: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSubscribeResponse {
    pub target_id: String,
    #[serde(flatten)]
    pub subscribe_response: SubscribeResponse,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriberRenegotiationResponse {
    pub target_id: String,
    pub sdp: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubsriberCandidateResponse {
    pub target_id: String,
    pub candidate: IceCandidate,
}
