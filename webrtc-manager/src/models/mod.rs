use std::{pin::Pin, sync::Arc};

use serde::Serialize;

pub type IceCandidateCallback =
    Arc<dyn Fn(IceCandidate) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
pub type RenegotiationCallback = Arc<dyn Fn(String) + Send + Sync>;
pub type JoinedCallback = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct WClient {
    pub participant_id: String,
    pub room_id: String,
}

#[derive(Clone)]
pub struct JoinRoomParams {
    pub sdp: String,
    pub participant_id: String,
    pub is_video_enabled: bool,
    pub is_audio_enabled: bool,
    pub is_e2ee_enabled: bool,
    pub callback: JoinedCallback,
    pub on_candidate: IceCandidateCallback,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRoomResponse {
    pub sdp: String,
    pub is_recording: bool,
}

#[derive(Clone)]
pub struct SubscribeParams {
    pub target_id: String,
    pub participant_id: String,
    pub on_negotiation_needed: RenegotiationCallback,
    pub on_candidate: IceCandidateCallback,
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_m_line_index: Option<u16>,
}
