use std::sync::{Arc, Mutex};

use tracing::info;
use uuid::Uuid;
use webrtc::{
    rtp_transceiver::rtp_codec::{RTCRtpCodecParameters, RTPCodecType},
    track::track_remote::TrackRemote,
};

use super::track::Track;

const K_VP8_CODEC: &str = "video/vp8";
const K_VP9_CODEC: &str = "video/vp9";
const K_H264_CODEC: &str = "video/h264";
const K_H265_CODEC: &str = "video/h265";
const K_AV1_CODEC: &str = "video/av1";

#[derive(Debug)]
pub struct Media {
    pub media_id: String,
    pub participant_id: String,
    pub tracks: Mutex<Vec<Track>>,
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
            tracks: Mutex::new(vec![]),
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

    pub fn add_track(&mut self, rtp_track: Arc<TrackRemote>, room_id: String) -> bool {
        let tracks = match self.tracks.lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        };

        let track_index = tracks
            .iter()
            .position(|track| track.track.id() == rtp_track.id());

        if track_index.is_some() {
            return false;
        }

        let track = Track::new(rtp_track.clone(), room_id, self.participant_id.clone());

        self.tracks.lock().unwrap().push(track);

        if rtp_track.kind() == RTPCodecType::Video {
            self.codec = rtp_track.codec().capability.mime_type;
        }

        info!(
            "[TRACK ADDED]: Info id: {} kind: {} codec: {}",
            rtp_track.id(),
            rtp_track.kind(),
            rtp_track.codec().capability.mime_type
        );

        true
    }

    pub fn set_screen_sharing(&mut self, is_enabled: bool) {
        if self.is_screen_sharing == is_enabled {
            return;
        }

        self.is_screen_sharing = is_enabled;

        if !is_enabled {
            self.remove_last_track();
        }
    }

    pub fn set_hand_rasing(&mut self, is_enabled: bool) {
        if self.is_hand_raising == is_enabled {
            return;
        }

        self.is_hand_raising = is_enabled;
    }

    fn remove_last_track(&mut self) {
        let mut tracks = self.tracks.lock().unwrap();
        tracks.pop().unwrap();
    }

    pub fn remove_all_tracks(&mut self) {
        let mut tracks = self.tracks.lock().unwrap();

        tracks.clear();
    }

    pub fn video_codecs(&self) -> Vec<RTCRtpCodecParameters> {
        match self.codec.as_str() {
            K_VP8_CODEC => vec![RTCRtpCodecParameters::default()],
            K_VP9_CODEC => vec![RTCRtpCodecParameters::default()],
            K_H264_CODEC => vec![RTCRtpCodecParameters::default()],
            K_H265_CODEC => vec![RTCRtpCodecParameters::default()],
            K_AV1_CODEC => vec![RTCRtpCodecParameters::default()],
            _ => vec![RTCRtpCodecParameters::default()],
        }
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
