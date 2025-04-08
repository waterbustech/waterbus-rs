use std::sync::Arc;

use serde::Serialize;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;

type IceCandidateCallback = Arc<dyn Fn(RTCIceCandidate) + Send + Sync>;

#[derive(Clone)]
pub struct JoinRoomParams {
    pub sdp: String,
    pub participant_id: String,
    pub is_video_enabled: bool,
    pub is_audio_enabled: bool,
    pub is_e2ee_enabled: bool,
    pub callback: Arc<dyn Fn() + Send + Sync>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRoomResponse {
    pub offer: String,
    pub is_recording: bool,
}

#[derive(Clone)]
pub struct SubscribeParams {
    pub target_id: String,
    pub participant_id: String,
    pub on_negotiation_needed: Arc<dyn Fn() + Send + Sync>,
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

pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_m_line_index: Option<u16>,
}
