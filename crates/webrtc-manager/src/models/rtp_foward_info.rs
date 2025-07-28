use std::sync::Arc;

use webrtc::rtp::packet::Packet;

use super::quality::TrackQuality;

#[derive(Debug, Clone)]
pub struct RtpForwardInfo {
    pub packet: Arc<Packet>,
    pub is_svc: bool,
    pub track_quality: TrackQuality,
}
