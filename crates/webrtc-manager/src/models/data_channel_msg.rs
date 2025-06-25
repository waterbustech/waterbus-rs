use crate::models::quality::TrackQuality;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackSubscribedMessage {
    pub track_id: String,
    pub subscribed_count: u32,
    pub quality: Option<TrackQuality>,
    pub timestamp: u64,
}
