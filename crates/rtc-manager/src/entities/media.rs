use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use parking_lot::RwLock;
use str0m::{
    Rtc,
    media::{KeyframeRequest, MediaData, MediaKind, Mid},
};
use tracing::warn;

use crate::{
    entities::subscriber::Subscriber,
    models::{connection_type::ConnectionType, streaming_protocol::StreamingProtocol},
};

#[derive(Debug, Clone)]
pub struct MediaState {
    pub camera_type: u8,
    pub video_enabled: bool,
    pub audio_enabled: bool,
    pub is_hand_raising: bool,
    pub is_e2ee_enabled: bool,
    pub is_screen_sharing: bool,
    pub screen_track_id: Option<String>,
    pub codec: String,
}

impl Default for MediaState {
    fn default() -> Self {
        Self {
            camera_type: 0,
            video_enabled: true,
            audio_enabled: true,
            is_hand_raising: false,
            is_e2ee_enabled: false,
            is_screen_sharing: false,
            screen_track_id: None,
            codec: "h264".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct TrackInfo {
    pub mid: Mid,
    pub kind: MediaKind,
    pub origin: String,
    pub last_keyframe_request: Option<Instant>,
}

impl TrackInfo {
    pub fn new(mid: Mid, kind: MediaKind, origin: String) -> Self {
        Self {
            mid,
            kind,
            origin,
            last_keyframe_request: None,
        }
    }
}

#[derive(Debug)]
pub struct Media {
    pub participant_id: String,
    pub rtc: Arc<RwLock<Rtc>>,
    pub connection_type: ConnectionType,
    pub state: Arc<RwLock<MediaState>>,
    pub tracks: Arc<DashMap<Mid, TrackInfo>>,
    pub subscribers: Arc<DashMap<String, Arc<Subscriber>>>,
    pub cached_sdp: Option<String>,
    pub streaming_protocol: StreamingProtocol,
    pub is_ipv6_supported: bool,
}

impl Media {
    pub fn new(
        participant_id: String,
        rtc: Arc<RwLock<Rtc>>,
        connection_type: ConnectionType,
        streaming_protocol: StreamingProtocol,
        is_ipv6_supported: bool,
    ) -> Self {
        Self {
            participant_id,
            rtc,
            connection_type,
            state: Arc::new(RwLock::new(MediaState::default())),
            tracks: Arc::new(DashMap::new()),
            subscribers: Arc::new(DashMap::new()),
            cached_sdp: None,
            streaming_protocol,
            is_ipv6_supported,
        }
    }

    pub fn cache_sdp(&mut self, sdp: String) {
        self.cached_sdp = Some(sdp);
    }

    pub fn get_sdp(&self) -> Option<String> {
        self.cached_sdp.clone()
    }

    pub fn add_track(&mut self, mid: Mid, kind: MediaKind) {
        let track_info = TrackInfo::new(mid, kind, self.participant_id.clone());
        self.tracks.insert(mid, track_info);
    }

    pub fn remove_track(&mut self, mid: Mid) {
        self.tracks.remove(&mid);
    }

    pub fn add_subscriber(&mut self, subscriber_id: String, subscriber: Arc<Subscriber>) {
        self.subscribers.insert(subscriber_id, subscriber);
    }

    pub fn remove_subscriber(&mut self, subscriber_id: &str) {
        self.subscribers.remove(subscriber_id);
    }

    pub fn get_subscriber(
        &self,
        subscriber_id: &str,
    ) -> Result<Arc<crate::entities::subscriber::Subscriber>, crate::errors::WebRTCError> {
        self.subscribers
            .get(subscriber_id)
            .map(|r| r.clone())
            .ok_or(crate::errors::WebRTCError::ParticipantNotFound)
    }

    pub fn forward_media_to_subscribers(&self, media_data: MediaData) {
        for subscriber in self.subscribers.iter() {
            if let Some(writer) = subscriber.rtc.write().writer(media_data.mid) {
                if let Err(e) = writer.write(
                    media_data.pt,
                    Instant::now(),
                    media_data.time,
                    &*media_data.data,
                ) {
                    warn!(
                        "Failed to forward media to subscriber {}: {:?}",
                        subscriber.participant_id, e
                    );
                }
            }
        }
    }

    pub fn handle_keyframe_request(&self, req: KeyframeRequest) {
        // Forward keyframe request to the appropriate track
        if let Some(track_info) = self.tracks.get(&req.mid) {
            // Check if we need to throttle the request
            if let Some(last_request) = track_info.last_keyframe_request {
                if last_request.elapsed().as_secs() < 1 {
                    return; // Throttle keyframe requests
                }
            }

            // Update the last keyframe request time
            if let Some(mut track_info) = self.tracks.get_mut(&req.mid) {
                track_info.last_keyframe_request = Some(Instant::now());
            }

            // Forward the keyframe request to subscribers
            for subscriber in self.subscribers.iter() {
                if let Some(mut writer) = subscriber.rtc.write().writer(req.mid) {
                    if let Err(e) = writer.request_keyframe(req.rid, req.kind) {
                        warn!(
                            "Failed to request keyframe from subscriber {}: {:?}",
                            subscriber.participant_id, e
                        );
                    }
                }
            }
        }
    }

    pub fn set_video_enabled(&mut self, enabled: bool) {
        let mut state = self.state.write();
        state.video_enabled = enabled;
    }

    pub fn set_audio_enabled(&mut self, enabled: bool) {
        let mut state = self.state.write();
        state.audio_enabled = enabled;
    }

    pub fn set_e2ee_enabled(&mut self, enabled: bool) {
        let mut state = self.state.write();
        state.is_e2ee_enabled = enabled;
    }

    pub fn set_camera_type(&mut self, camera_type: u8) {
        let mut state = self.state.write();
        state.camera_type = camera_type;
    }

    pub fn set_screen_sharing(&mut self, enabled: bool, screen_track_id: Option<String>) {
        let mut state = self.state.write();
        state.is_screen_sharing = enabled;
        state.screen_track_id = screen_track_id;
    }

    pub fn set_hand_raising(&mut self, enabled: bool) {
        let mut state = self.state.write();
        state.is_hand_raising = enabled;
    }

    pub fn get_hls_urls(&self) -> Vec<String> {
        // This would be implemented based on your HLS streaming logic
        vec![]
    }

    pub fn stop(&mut self) {
        // Clean up resources
        self.subscribers.clear();
        self.tracks.clear();
    }
}
