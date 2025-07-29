use std::{collections::HashMap, fs, path::Path, sync::Arc};

use dashmap::DashMap;
use egress_manager::egress::{hls_writer::HlsWriter, moq_writer::MoQWriter};
use nanoid::nanoid;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::{debug, info};
use str0m::media::{Mid, MediaKind, Direction};

use crate::models::{
    data_channel_msg::TrackSubscribedMessage,
    params::{AddTrackResponse, TrackMutexWrapper},
    streaming_protocol::StreamingProtocol,
};

use super::track::Track;

pub type TrackSubscribedCallback = Arc<dyn Fn(TrackSubscribedMessage) + Send + Sync>;

/// Media is a media that is used to manage the media of the participant
#[derive(Clone)]
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
    // str0m specific fields
    pub media_mids: Arc<RwLock<HashMap<MediaKind, Mid>>>,
}

#[derive(Debug, Clone)]
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
            media_mids: Arc::new(RwLock::new(HashMap::new())),
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

    /// Add a media Mid for tracking
    pub fn add_media_mid(&self, kind: MediaKind, mid: Mid) {
        let mut mids = self.media_mids.write();
        mids.insert(kind, mid);
    }

    /// Get the Mid for a specific media kind
    pub fn get_media_mid(&self, kind: MediaKind) -> Option<Mid> {
        let mids = self.media_mids.read();
        mids.get(&kind).copied()
    }

    /// Add track - simplified for str0m
    pub fn add_track(&self, track_id: String, track: Arc<RwLock<Track>>) -> AddTrackResponse {
        info!("Adding track {} to media {}", track_id, self.media_id);
        
        // Store the track
        self.tracks.insert(track_id.clone(), track.clone());
        
        // Notify subscribers if callback is set
        if let Some(callback) = &self.track_subscribed_callback {
            let msg = TrackSubscribedMessage {
                track_id: track_id.clone(),
                participant_id: self.participant_id.clone(),
            };
            callback(msg);
        }

        AddTrackResponse::AddTrackSuccess(track)
    }

    /// Remove all tracks
    pub fn remove_all_tracks(&self) {
        self.tracks.clear();
    }

    /// Get HLS URLs - simplified
    pub fn get_hls_urls(&self) -> Vec<String> {
        let writers = self.hls_writers.read();
        writers.values().map(|writer| writer.get_url()).collect()
    }

    /// Stop media processing
    pub fn stop(&self) {
        info!("Stopping media for participant {}", self.participant_id);
        
        // Clear tracks
        self.remove_all_tracks();
        
        // Clear HLS writers
        let mut writers = self.hls_writers.write();
        writers.clear();
    }

    /// Set track subscribed callback
    pub fn set_track_subscribed_callback(&mut self, callback: TrackSubscribedCallback) {
        self.track_subscribed_callback = Some(callback);
    }

    /// Get participant ID
    pub fn get_participant_id(&self) -> &str {
        &self.participant_id
    }

    /// Initialize HLS writer (if needed for HLS streaming)
    pub async fn initialize_hls_writer(&mut self) -> Result<(), anyhow::Error> {
        // Simplified HLS writer initialization
        info!("Initializing HLS writer for participant {}", self.participant_id);
        Ok(())
    }
}
