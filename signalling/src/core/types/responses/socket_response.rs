use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]

pub struct ParticipantHasLeftResponse {
    pub target_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRoomResponse {
    pub sdp: String,
    pub is_recording: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeParticipantResponse {
    pub target_id: String,
    #[serde(flatten)]
    pub subscribe_response: SubscribeResponse,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeResponse {
    pub offer: String,
    pub camera_type: u8,
    pub video_enabled: bool,
    pub audio_enabled: bool,
    pub is_screen_sharing: bool,
    pub is_hand_raising: bool,
    pub is_e2ee_enabled: bool,
    pub video_codec: String,
    pub screen_track_id: Option<String>,
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
    pub screen_track_id: Option<String>,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_m_line_index: Option<u32>,
}
