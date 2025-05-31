use std::{fs, path::Path, sync::Arc};

use dashmap::DashMap;
use egress_manager::egress::{hls_writer::HlsWriter, moq_writer::MoQWriter};
use nanoid::nanoid;
use parking_lot::RwLock;
use tracing::{debug, info};
use webrtc::{rtp_transceiver::rtp_codec::RTPCodecType, track::track_remote::TrackRemote};

use crate::models::{AddTrackResponse, TrackMutexWrapper};

use super::track::Track;

#[derive(Debug)]
pub struct Media {
    pub media_id: String,
    pub participant_id: String,
    pub tracks: Arc<DashMap<String, TrackMutexWrapper>>,
    pub state: Arc<RwLock<MediaState>>,
    hls_writer: Option<Arc<HlsWriter>>,
    moq_writer: Option<Arc<MoQWriter>>,
    output_dir: String,
    sdp: Option<String>,
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
        let output_dir = format!("./hls/{}", publisher_id);

        if !Path::new(&output_dir).exists() {
            fs::create_dir_all(&output_dir).unwrap();
        }

        Self {
            media_id: format!("m_{}", nanoid!(12)),
            participant_id: publisher_id,
            tracks: Arc::new(DashMap::new()),
            hls_writer: None,
            moq_writer: None,
            output_dir,
            sdp: None,
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

    pub async fn initialize_hls_writer(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let hls_writer = HlsWriter::new(&self.output_dir, self.participant_id.clone()).await?;
        self.hls_writer = Some(Arc::new(hls_writer));
        Ok(())
    }

    pub fn initialize_moq_writer(&mut self) -> Result<(), anyhow::Error> {
        let moq_writer = MoQWriter::new(&self.participant_id.clone())?;
        self.moq_writer = Some(Arc::new(moq_writer));
        Ok(())
    }

    pub fn cache_sdp(&mut self, sdp: String) {
        self.sdp = Some(sdp);
    }

    pub fn get_sdp(&mut self) -> Option<String> {
        let sdp = self.sdp.clone();

        self.sdp = None;

        sdp
    }

    // Alternative: Static method that creates and initializes everything
    pub async fn new_with_hls(
        publisher_id: String,
        is_video_enabled: bool,
        is_audio_enabled: bool,
        is_e2ee_enabled: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut media = Self::new(
            publisher_id,
            is_video_enabled,
            is_audio_enabled,
            is_e2ee_enabled,
        );
        media.initialize_hls_writer().await?;
        Ok(media)
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

        if rtp_track.kind() == RTPCodecType::Video {
            let codec = match rtp_track
                .codec()
                .capability
                .mime_type
                .to_lowercase()
                .as_str()
            {
                s if s.contains("vp8") => "vp8",
                s if s.contains("vp9") => "vp9",
                s if s.contains("av1") => "av1",
                s if s.contains("h264") => "h264",
                _ => "h264",
            };

            if let Some(hls_writer) = &self.hls_writer {
                hls_writer.set_video_codec(codec);
            }

            if let Some(moq_writer) = &self.moq_writer {
                moq_writer.set_video_codec(codec);
            }
        }

        let new_track = Arc::new(tokio::sync::RwLock::new(Track::new(
            rtp_track.clone(),
            room_id,
            self.participant_id.clone(),
            self.hls_writer.clone(),
            self.moq_writer.clone(),
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
        for entry in self.tracks.iter() {
            let track_mutex = entry.value().clone();

            tokio::spawn(async move {
                let mut track = track_mutex.write().await;
                track.stop();
            });
        }

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
        self.remove_all_tracks();

        if let Some(writer) = &self.hls_writer {
            writer.stop();
        }
        if let Some(writer) = &self.moq_writer {
            writer.stop();
        }

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
