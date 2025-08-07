use crate::models::quality::TrackQuality;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackQualityRequest {
    pub track_id: String,
    pub quality: TrackQuality,
    pub timestamp: u64,
}

impl TrackQualityRequest {
    pub fn new(track_id: String, quality: TrackQuality) -> Self {
        Self {
            track_id,
            quality,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
}
