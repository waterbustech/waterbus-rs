use std::sync::Arc;

use dashmap::DashMap;
use webrtc::rtp::packet::Packet;

use super::quality::TrackQuality;

#[derive(Debug, Clone)]
pub struct RtpForwardInfo {
    pub packet: Arc<Packet>,
    pub acceptable_map: Arc<DashMap<(TrackQuality, TrackQuality), bool>>,
    pub is_svc: bool,
    pub is_simulcast: bool,
    pub track_quality: TrackQuality,
}
