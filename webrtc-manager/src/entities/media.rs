use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;
use webrtc::{rtp_transceiver::rtp_codec::RTPCodecType, track::track_remote::TrackRemote};

use super::track::Track;

#[derive(Debug)]
pub struct Media {
    pub media_id: String,
    pub participant_id: String,
    pub tracks: Arc<Mutex<Vec<Track>>>,
    pub video_enabled: bool,
    pub audio_enabled: bool,
    pub is_e2ee_enabled: bool,
    pub is_screen_sharing: bool,
    pub is_hand_raising: bool,
    pub camera_type: u8,
    pub codec: String,
}

impl Media {
    pub fn new(
        publisher_id: String,
        is_video_enabled: bool,
        is_audio_enabled: bool,
        is_e2ee_enabled: bool,
    ) -> Self {
        Media {
            media_id: format!("m_{}", Uuid::new_v4()),
            participant_id: publisher_id,
            tracks: Arc::new(Mutex::new(vec![])),
            video_enabled: is_video_enabled,
            audio_enabled: is_audio_enabled,
            is_e2ee_enabled,
            is_screen_sharing: false,
            is_hand_raising: false,
            camera_type: 0, // 0: front, 1: rear
            codec: String::new(),
        }
    }

    pub fn stop(&self) {}

    pub async fn add_track(&mut self, rtp_track: Arc<TrackRemote>, room_id: String) -> bool {
        {
            let tracks = self.tracks.lock().await;

            let track_index = tracks.iter().position(|track| {
                track.track.id() == rtp_track.id() && track.track.rid() == rtp_track.rid()
            });

            if track_index.is_some() {
                return false;
            }
        }

        let track = Track::new(rtp_track.clone(), room_id, self.participant_id.clone());

        self.tracks.lock().await.push(track);

        if rtp_track.kind() == RTPCodecType::Video {
            self.codec = rtp_track.codec().capability.mime_type;
        }

        info!(
            "[Track added]: Info id: {} kind: {} codec: {}, rid: {}, stream_id: {}",
            rtp_track.id(),
            rtp_track.kind(),
            rtp_track.codec().capability.mime_type,
            rtp_track.rid(),
            rtp_track.stream_id(),
        );

        true
    }

    pub async fn set_screen_sharing(&mut self, is_enabled: bool) {
        if self.is_screen_sharing == is_enabled {
            return;
        }

        self.is_screen_sharing = is_enabled;

        if !is_enabled {
            self.remove_last_track().await;
        }
    }

    pub fn set_hand_rasing(&mut self, is_enabled: bool) {
        if self.is_hand_raising == is_enabled {
            return;
        }

        self.is_hand_raising = is_enabled;
    }

    async fn remove_last_track(&mut self) {
        let mut tracks = self.tracks.lock().await;
        tracks.pop().unwrap();
    }

    pub async fn remove_all_tracks(&mut self) {
        let mut tracks = self.tracks.lock().await;

        tracks.clear();
    }

    pub fn set_camera_type(&mut self, camera_type: u8) {
        self.camera_type = camera_type;
    }

    pub fn set_video_enabled(&mut self, is_enabled: bool) {
        self.video_enabled = is_enabled;
    }

    pub fn set_audio_enabled(&mut self, is_enabled: bool) {
        self.audio_enabled = is_enabled;
    }

    pub fn set_e2ee_enabled(&mut self, is_enabled: bool) {
        self.is_e2ee_enabled = is_enabled;
    }

    pub fn info(&self) -> MediaInfo {
        MediaInfo {
            publisher_id: self.participant_id.clone(),
        }
    }
}

pub struct MediaInfo {
    pub publisher_id: String,
}
