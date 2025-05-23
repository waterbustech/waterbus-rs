use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;
use tracing::info;
use uuid::Uuid;
use webrtc::{rtp_transceiver::rtp_codec::RTPCodecType, track::track_remote::TrackRemote};

use crate::models::{AddTrackResponse, TrackMutexWrapper};

use super::track::Track;

#[derive(Debug)]
pub struct Media {
    pub media_id: String,
    pub participant_id: String,
    pub tracks: Arc<DashMap<String, TrackMutexWrapper>>,
    pub state: Arc<RwLock<MediaState>>,
}

#[derive(Debug)]
pub struct MediaState {
    pub video_enabled: bool,
    pub audio_enabled: bool,
    pub is_e2ee_enabled: bool,
    pub is_screen_sharing: bool,
    pub is_hand_raising: bool,
    pub camera_type: u8,
    pub codec: String,
    pub screen_track_id: Option<String>,
}

impl Media {
    pub fn new(
        publisher_id: String,
        is_video_enabled: bool,
        is_audio_enabled: bool,
        is_e2ee_enabled: bool,
    ) -> Self {
        Self {
            media_id: format!("m_{}", Uuid::new_v4()),
            participant_id: publisher_id,
            tracks: Arc::new(DashMap::new()),
            state: Arc::new(RwLock::new(MediaState {
                video_enabled: is_video_enabled,
                audio_enabled: is_audio_enabled,
                is_e2ee_enabled,
                is_screen_sharing: false,
                is_hand_raising: false,
                camera_type: 0,
                codec: String::new(),
                screen_track_id: None,
            })),
        }
    }

    pub async fn add_track(
        &self,
        rtp_track: Arc<TrackRemote>,
        room_id: String,
    ) -> AddTrackResponse {
        if let Some(existing_track_arc) = self.tracks.get(&rtp_track.id()) {
            let mut track_guard = existing_track_arc.write().await;
            track_guard.add_track(rtp_track.clone());

            if rtp_track.kind() == RTPCodecType::Video {
                let mut state = self.state.write();
                state.codec = rtp_track.codec().capability.mime_type;
            }

            self._log_track_added(rtp_track);
            return AddTrackResponse::AddSimulcastTrackSuccess(existing_track_arc.clone());
        }

        let new_track = Arc::new(tokio::sync::RwLock::new(Track::new(
            rtp_track.clone(),
            room_id,
            self.participant_id.clone(),
        )));

        if rtp_track.kind() == RTPCodecType::Video {
            let mut state = self.state.write();
            state.codec = rtp_track.codec().capability.mime_type;
        }

        self.tracks.insert(rtp_track.id(), new_track.clone());

        self._log_track_added(rtp_track);

        AddTrackResponse::AddTrackSuccess(new_track)
    }

    pub fn set_screen_sharing(&self, is_enabled: bool, screen_track_id: Option<String>) {
        let mut state = self.state.write();
        if state.is_screen_sharing != is_enabled {
            state.is_screen_sharing = is_enabled;

            if !is_enabled {
                drop(state); // Unlock before async task

                self.remove_screen_track();
            } else {
                state.screen_track_id = screen_track_id;
            }
        }
    }

    pub fn set_hand_rasing(&self, is_enabled: bool) {
        let mut state = self.state.write();
        state.is_hand_raising = is_enabled;
    }

    fn remove_screen_track(&self) {
        let screen_track_id_opt = {
            let state = self.state.read();
            state.screen_track_id.clone()
        };

        if let Some(screen_track_id) = screen_track_id_opt {
            let removed = self.tracks.remove(&screen_track_id);
            if removed.is_some() {
                info!("[screen_track_removed]: id: {}", screen_track_id);
            } else {
                info!(
                    "[screen_track_remove_failed]: id not found: {}",
                    screen_track_id
                );
            }

            // Clear the screen_track_id
            let mut state = self.state.write();
            state.screen_track_id = None;
        }
    }

    pub fn remove_all_tracks(&self) {
        self.tracks.clear();
    }

    pub fn set_camera_type(&self, camera_type: u8) {
        self.state.write().camera_type = camera_type;
    }

    pub fn set_video_enabled(&self, is_enabled: bool) {
        self.state.write().video_enabled = is_enabled;
    }

    pub fn set_audio_enabled(&self, is_enabled: bool) {
        self.state.write().audio_enabled = is_enabled;
    }

    pub fn set_e2ee_enabled(&self, is_enabled: bool) {
        self.state.write().is_e2ee_enabled = is_enabled;
    }

    pub fn stop(&self) {
        for entry in self.tracks.iter() {
            let track_mutex = entry.value().clone();

            tokio::spawn(async move {
                let mut track = track_mutex.write().await;
                track.stop();
            });
        }

        self.remove_all_tracks();

        {
            let mut state = self.state.write();
            state.screen_track_id = None;
            state.video_enabled = false;
            state.audio_enabled = false;
            state.is_screen_sharing = false;
            state.is_hand_raising = false;
            state.camera_type = 0;
            state.codec.clear();
        }
    }

    fn _log_track_added(&self, rtp_track: Arc<TrackRemote>) {
        let rid = if rtp_track.kind() == RTPCodecType::Audio {
            "audio"
        } else if rtp_track.rid().is_empty() {
            "none"
        } else {
            rtp_track.rid()
        };

        info!(
            "[track_added]: id: {} kind: {} codec: {}, rid: {}, stream_id: {}, ssrc: {}",
            rtp_track.id(),
            rtp_track.kind(),
            rtp_track.codec().capability.mime_type,
            rid,
            rtp_track.stream_id(),
            rtp_track.ssrc(),
        );
    }
}
