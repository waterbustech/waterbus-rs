use serde::{Deserialize, Serialize};
use webrtc::rtp::codecs::vp9::Vp9Packet;

#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum TrackQuality {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
}

impl TrackQuality {
    pub fn from_str(s: &str) -> TrackQuality {
        match s {
            "q" => TrackQuality::Low,
            "h" => TrackQuality::Medium,
            "f" => TrackQuality::High,
            _ => TrackQuality::None,
        }
    }

    pub fn from_u8(value: u8) -> TrackQuality {
        match value {
            1 => TrackQuality::Low,
            2 => TrackQuality::Medium,
            3 => TrackQuality::High,
            _ => TrackQuality::None,
        }
    }

    pub fn as_u8(&self) -> u8 {
        self.clone() as u8
    }

    // Convert TrackQuality to SVC layer IDs for VP9/AV1
    fn quality_to_svc_layers(&self) -> (u8, u8) {
        match self {
            TrackQuality::Low => (0, 0),
            TrackQuality::Medium => (1, 1),
            TrackQuality::High => (2, 2),
            TrackQuality::None => (0, 0),
        }
    }

    // Check if an SVC packet should be forwarded based on desired quality
    pub fn should_forward_vp9_svc(&self, vp9_packet: &Vp9Packet) -> bool {
        if !vp9_packet.l || vp9_packet.tid == 0 {
            return true;
        }

        let (desired_spatial_id, _) = self.quality_to_svc_layers();

        let forward = vp9_packet.sid == desired_spatial_id;

        forward
    }
}
