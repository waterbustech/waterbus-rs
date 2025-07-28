use std::{collections::HashMap, fs, path::Path, sync::Arc};

use dashmap::DashMap;
use egress_manager::egress::{hls_writer::HlsWriter, moq_writer::MoQWriter};
use nanoid::nanoid;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::{debug, info};
use webrtc::{rtp_transceiver::rtp_codec::RTPCodecType, track::track_remote::TrackRemote};

use crate::models::{
    data_channel_msg::TrackSubscribedMessage,
    params::{AddTrackResponse, TrackMutexWrapper},
    streaming_protocol::StreamingProtocol,
};

use super::track::Track;

pub type TrackSubscribedCallback = Arc<dyn Fn(TrackSubscribedMessage) + Send + Sync>;

/// Media is a media that is used to manage the media of the participant
pub struct Media {
    pub media_id: String,
    pub participant_id: String,
    pub tracks: Arc<DashMap<String, TrackMutexWrapper>>,
    pub state: Arc<RwLock<MediaState>>,
    pub moq_writer: Option<Arc<MoQWriter>>,
    pub sdp: Option<String>,
    pub hls_writers: Arc<RwLock<HashMap<String, Arc<HlsWriter>>>>,
    pub track_subscribed_callback: Option<TrackSubscribedCallback>,
    pub track_event_sender: Option<mpsc::UnboundedSender<TrackSubscribedMessage>>,
    pub keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
    pub streaming_protocol: StreamingProtocol,
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
    /// Create a new Media
    ///
    /// # Arguments
    ///
    /// * `publisher_id` - The id of the publisher
    /// * `is_video_enabled` - Whether the video is enabled
    pub fn new(
        publisher_id: String,
        is_video_enabled: bool,
        is_audio_enabled: bool,
        is_e2ee_enabled: bool,
        streaming_protocol: StreamingProtocol,
    ) -> Self {
        Self {
            media_id: format!("m_{}", nanoid!(12)),
            participant_id: publisher_id,
            tracks: Arc::new(DashMap::new()),
            hls_writers: Arc::new(RwLock::new(HashMap::new())),
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
            track_subscribed_callback: None,
            track_event_sender: None,
            keyframe_request_callback: None,
            moq_writer: None,
            sdp: None,
            streaming_protocol,
        }
    }

    /// Initialize the moq writer
    ///
    /// # Arguments
    ///
    /// * `self` - The Media
    ///
    pub fn initialize_moq_writer(&mut self) -> Result<(), anyhow::Error> {
        let moq_writer = MoQWriter::new(&self.participant_id.clone())?;
        self.moq_writer = Some(Arc::new(moq_writer));
        Ok(())
    }

    /// Cache the sdp incase peer to peer connection
    ///
    /// # Arguments
    ///
    /// * `sdp` - The sdp to cache
    ///
    #[inline]
    pub fn cache_sdp(&mut self, sdp: String) {
        self.sdp = Some(sdp);
    }

    /// Get the sdp
    ///
    /// # Arguments
    ///
    /// * `self` - The Media
    ///
    #[inline]
    pub fn get_sdp(&mut self) -> Option<String> {
        let sdp = self.sdp.clone();

        self.sdp = None;

        sdp
    }

    /// Add a new track to the Media
    ///
    /// # Arguments
    ///
    /// * `rtp_track` - The rtp track to add
    /// * `room_id` - The id of the room
    ///
    pub fn add_track(&self, rtp_track: Arc<TrackRemote>, room_id: String) -> AddTrackResponse {
        if let Some(existing_track_arc) = self.tracks.get(&rtp_track.id()) {
            let mut track_guard = existing_track_arc.write();
            track_guard.add_track(rtp_track.clone());

            if rtp_track.kind() == RTPCodecType::Video {
                let mut state = self.state.write();
                state.codec = rtp_track.codec().capability.mime_type;
            }

            self._log_track_added(rtp_track);
            return AddTrackResponse::AddSimulcastTrackSuccess(existing_track_arc.clone());
        }

        let mut hls_writer = None;

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

            if self.streaming_protocol == StreamingProtocol::HLS {
                hls_writer = self.add_track_to_hls_writer(rtp_track.clone());
            }

            if let Some(moq_writer) = &self.moq_writer {
                moq_writer.set_video_codec(codec);
            }
        }

        let new_track = Arc::new(RwLock::new(Track::new(
            rtp_track.clone(),
            room_id,
            self.participant_id.clone(),
            hls_writer,
            self.moq_writer.clone(),
            self.keyframe_request_callback.clone(),
        )));

        if rtp_track.kind() == RTPCodecType::Video {
            let mut state = self.state.write();
            state.codec = rtp_track.codec().capability.mime_type;
        }

        self.tracks.insert(rtp_track.id(), new_track.clone());

        self._log_track_added(rtp_track);

        AddTrackResponse::AddTrackSuccess(new_track)
    }

    /// Add a new track to the hls writer
    ///
    /// # Arguments
    ///
    /// * `rtp_track` - The rtp track to add
    ///
    #[inline]
    pub fn add_track_to_hls_writer(&self, rtp_track: Arc<TrackRemote>) -> Option<Arc<HlsWriter>> {
        if self.streaming_protocol == StreamingProtocol::HLS {
            let hls_writer = self._initialize_hls_writer(&rtp_track.id());

            if let Ok(hls_writer) = hls_writer {
                hls_writer.set_video_codec(&rtp_track.codec().capability.mime_type);

                return Some(hls_writer);
            }
        }

        None
    }

    /// Initialize the hls writer
    ///
    /// # Arguments
    ///
    /// * `track_id` - The id of the track
    ///
    fn _initialize_hls_writer(&self, track_id: &str) -> Result<Arc<HlsWriter>, anyhow::Error> {
        let output_dir = format!("./hls/{}/{}", self.participant_id, track_id);

        if !Path::new(&output_dir).exists() {
            fs::create_dir_all(&output_dir).unwrap();
        }

        let prefix_path = format!("{}/{}", self.participant_id, track_id);

        let hls_writer = HlsWriter::new(&output_dir, &prefix_path)?;
        let hls_writer_arc = Arc::new(hls_writer);
        self.hls_writers
            .write()
            .insert(self.participant_id.clone(), hls_writer_arc.clone());
        Ok(hls_writer_arc)
    }

    /// Set the screen sharing
    ///
    /// # Arguments
    ///
    /// * `is_enabled` - Whether the screen sharing is enabled
    /// * `screen_track_id` - The id of the screen track
    ///
    #[inline]
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

    /// Set the hand raising
    ///
    /// # Arguments
    ///
    /// * `is_enabled` - Whether the hand raising is enabled
    ///
    #[inline]
    pub fn set_hand_rasing(&self, is_enabled: bool) {
        let mut state = self.state.write();
        state.is_hand_raising = is_enabled;
    }

    /// Remove the screen track
    ///
    /// # Arguments
    ///
    /// * `self` - The Media
    ///
    #[inline]
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

    /// Remove all tracks
    ///
    /// # Arguments
    ///
    /// * `self` - The Media
    ///
    #[inline]
    pub fn remove_all_tracks(&self) {
        for entry in self.tracks.iter() {
            let track_mutex = entry.value().clone();

            let mut track = track_mutex.write();
            track.stop();
        }

        self.tracks.clear();
    }

    /// Set the camera type
    ///
    /// # Arguments
    ///
    /// * `camera_type` - The camera type
    ///
    #[inline]
    pub fn set_camera_type(&self, camera_type: u8) {
        self.state.write().camera_type = camera_type;
    }

    /// Set the video enabled
    ///
    /// # Arguments
    ///
    /// * `is_enabled` - Whether the video is enabled
    ///
    #[inline]
    pub fn set_video_enabled(&self, is_enabled: bool) {
        self.state.write().video_enabled = is_enabled;
    }

    /// Set the audio enabled
    ///
    /// # Arguments
    ///
    /// * `is_enabled` - Whether the audio is enabled
    ///
    #[inline]
    pub fn set_audio_enabled(&self, is_enabled: bool) {
        self.state.write().audio_enabled = is_enabled;
    }

    /// Set the e2ee enabled
    ///
    /// # Arguments
    ///
    /// * `is_enabled` - Whether the e2ee is enabled
    ///
    #[inline]
    pub fn set_e2ee_enabled(&self, is_enabled: bool) {
        self.state.write().is_e2ee_enabled = is_enabled;
    }

    /// Stop the Media
    ///
    /// # Arguments
    ///
    /// * `self` - The Media
    ///
    pub fn stop(&self) {
        self.remove_all_tracks();

        let hls_writers = self.hls_writers.clone();
        tokio::spawn(async move {
            let mut hls_writers = hls_writers.write();
            for writer in hls_writers.values() {
                writer.stop();
            }
            hls_writers.clear();
        });

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

    /// Log the track added
    ///
    /// # Arguments
    ///
    /// * `rtp_track` - The rtp track
    ///
    #[inline]
    fn _log_track_added(&self, rtp_track: Arc<TrackRemote>) {
        let rid = if rtp_track.kind() == RTPCodecType::Audio {
            "audio"
        } else if rtp_track.rid().is_empty() {
            "none"
        } else {
            rtp_track.rid()
        };

        debug!(
            "[track_added]: id: {} kind: {} codec: {}, rid: {}, stream_id: {}, ssrc: {}",
            rtp_track.id(),
            rtp_track.kind(),
            rtp_track.codec().capability.mime_type,
            rid,
            rtp_track.stream_id(),
            rtp_track.ssrc(),
        );
    }

    /// Get the hls urls
    ///
    /// # Arguments
    ///
    /// * `self` - The Media
    ///
    #[inline]
    pub fn get_hls_urls(&self) -> Vec<String> {
        self.hls_writers
            .read()
            .values()
            .map(|writer| writer.hls_url.clone())
            .collect()
    }
}
