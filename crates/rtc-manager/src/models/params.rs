use std::{pin::Pin, sync::Arc, future::Future};
use parking_lot::RwLock;
use serde::Serialize;

use crate::models::streaming_protocol::StreamingProtocol;
use super::connection_type::ConnectionType;

pub type IceCandidateCallback =
    Arc<dyn Fn(IceCandidate) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
pub type RenegotiationCallback =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
pub type JoinedCallback =
    Arc<dyn Fn(bool) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct RtcManagerConfigs {
    pub public_ip: String,
    pub port_min: u16,
    pub port_max: u16,
}

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
    pub total_tracks: u8,
    pub connection_type: ConnectionType,
    pub callback: JoinedCallback,
    pub on_candidate: IceCandidateCallback,
    pub streaming_protocol: StreamingProtocol,
    pub is_ipv6_supported: bool,
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
    pub is_ipv6_supported: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeResponse {
    pub sdp: String,
    pub offer: String,
    pub camera_type: u8,
    pub video_enabled: bool,
    pub audio_enabled: bool,
    pub is_screen_sharing: bool,
    pub is_hand_raising: bool,
    pub is_e2ee_enabled: bool,
    pub video_codec: String,
    pub screen_track_id: String,
}

#[derive(Clone)]
pub struct SubscribeHlsLiveStreamParams {
    pub target_id: String,
    pub participant_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeHlsLiveStreamResponse {
    pub playlist_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_m_line_index: Option<u16>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddTrackResponse {
    pub track_id: String,
}

// Type alias for track wrapper - using Arc<RwLock<T>> pattern
pub type TrackMutexWrapper = Arc<RwLock<crate::entities::track::Track>>;
