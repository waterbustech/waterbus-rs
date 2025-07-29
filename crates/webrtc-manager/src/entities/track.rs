use bytes::BytesMut;
use dashmap::DashMap;
use egress_manager::egress::hls_writer::HlsWriter;
use egress_manager::egress::moq_writer::MoQWriter;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::debug;
use str0m::media::{Mid, MediaKind, Direction, Rid};

use crate::errors::WebRTCError;
use crate::models::quality::TrackQuality;
use crate::models::rtp_foward_info::RtpForwardInfo;
use crate::utils::multicast_sender::{MulticastSender, MulticastSenderImpl};

use super::forward_track::ForwardTrack;

#[derive(Clone, PartialEq)]
pub enum CodecType {
    H264,
    VP8,
    VP9,
    AV1,
    Other,
}

/// Track is a track that is used to forward media data in str0m
/// It represents a media stream (audio or video) from a participant
#[derive(Clone)]
pub struct Track {
    pub id: String,
    pub room_id: String,
    pub participant_id: String,
    pub is_simulcast: Arc<AtomicBool>,
    pub is_svc: bool,
    pub codec_type: CodecType,
    pub stream_id: String,
    pub kind: MediaKind,
    pub mid: Mid,
    pub rid: Option<Rid>,
    pub forward_tracks: Arc<DashMap<String, Arc<ForwardTrack<MulticastSenderImpl>>>>,
    pub ssrc: u32,
    rtp_multicast: Arc<MulticastSenderImpl>,
    keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
    pub quality: TrackQuality,
}

impl Track {
    /// Create a new Track for str0m
    pub fn new(
        id: String,
        room_id: String,
        participant_id: String,
        kind: MediaKind,
        mid: Mid,
        rid: Option<Rid>,
        hls_writer: Option<Arc<HlsWriter>>,
        moq_writer: Option<Arc<MoQWriter>>,
        keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
    ) -> Self {
        let multicast_sender = MulticastSenderImpl::new(4096); // Buffer size

        Self {
            id: id.clone(),
            room_id,
            participant_id,
            is_simulcast: Arc::new(AtomicBool::new(false)),
            is_svc: false,
            codec_type: CodecType::H264, // Default codec
            stream_id: id,
            kind,
            mid,
            rid,
            forward_tracks: Arc::new(DashMap::new()),
            ssrc: 0, // Will be set when media starts
            rtp_multicast: Arc::new(multicast_sender),
            keyframe_request_callback,
            quality: TrackQuality::High,
        }
    }

    /// Get track ID
    pub fn id(&self) -> String {
        self.id.clone()
    }

    /// Get track kind
    pub fn kind(&self) -> MediaKind {
        self.kind
    }

    /// Get Mid
    pub fn mid(&self) -> Mid {
        self.mid
    }

    /// Get RID (for simulcast)
    pub fn rid(&self) -> Option<Rid> {
        self.rid
    }

    /// Add a forward track for a subscriber
    pub fn add_forward_track(&self, user_id: &str, forward_track: Arc<ForwardTrack<MulticastSenderImpl>>) {
        self.forward_tracks.insert(user_id.to_string(), forward_track);
    }

    /// Remove a forward track
    pub fn remove_forward_track(&self, user_id: &str) {
        self.forward_tracks.remove(user_id);
    }

    /// Create a new forward track for a subscriber
    pub fn new_forward_track(&self, user_id: &str, ssrc: u32) -> Result<Arc<ForwardTrack<MulticastSenderImpl>>, WebRTCError> {
        let forward_track = ForwardTrack::new(
            self.id.clone(),
            self.participant_id.clone(),
            user_id.to_string(),
            self.kind,
            Arc::clone(&self.rtp_multicast),
        )?;
        
        let forward_track_arc = Arc::new(forward_track);
        self.add_forward_track(user_id, Arc::clone(&forward_track_arc));
        
        Ok(forward_track_arc)
    }

    /// Set codec type based on mime type
    pub fn set_codec_from_mime(&mut self, mime_type: &str) {
        self.codec_type = match mime_type.to_lowercase().as_str() {
            s if s.contains("h264") => CodecType::H264,
            s if s.contains("vp8") => CodecType::VP8,
            s if s.contains("vp9") => CodecType::VP9,
            s if s.contains("av1") => CodecType::AV1,
            _ => CodecType::Other,
        };
    }

    /// Set simulcast mode
    pub fn set_simulcast(&self, enabled: bool) {
        self.is_simulcast.store(enabled, Ordering::Relaxed);
    }

    /// Check if simulcast is enabled
    pub fn is_simulcast(&self) -> bool {
        self.is_simulcast.load(Ordering::Relaxed)
    }

    /// Set track quality
    pub fn set_quality(&mut self, quality: TrackQuality) {
        self.quality = quality;
    }

    /// Get track quality
    pub fn get_quality(&self) -> TrackQuality {
        self.quality
    }

    /// Set SSRC
    pub fn set_ssrc(&mut self, ssrc: u32) {
        self.ssrc = ssrc;
    }

    /// Get SSRC
    pub fn get_ssrc(&self) -> u32 {
        self.ssrc
    }

    /// Request keyframe for this track
    pub fn request_keyframe(&self) {
        if let Some(callback) = &self.keyframe_request_callback {
            callback(self.ssrc);
        }
    }

    /// Process incoming media data (str0m-style)
    pub fn process_media_data(&self, data: &[u8], timestamp: u64) {
        // Forward the data to all subscribers
        for entry in self.forward_tracks.iter() {
            let forward_track = entry.value();
            if let Err(e) = forward_track.send_data(data, timestamp) {
                debug!("Failed to forward data to {}: {:?}", entry.key(), e);
            }
        }
    }

    /// Stop the track
    pub fn stop(&self) {
        // Clear forward tracks
        self.forward_tracks.clear();
        
        debug!("Track {} stopped", self.id);
    }

    /// Get participant ID
    pub fn get_participant_id(&self) -> &str {
        &self.participant_id
    }

    /// Get room ID
    pub fn get_room_id(&self) -> &str {
        &self.room_id
    }

    /// Check if track is active
    pub fn is_active(&self) -> bool {
        !self.forward_tracks.is_empty()
    }

    /// Get forward track for user
    pub fn get_forward_track(&self, user_id: &str) -> Option<Arc<ForwardTrack<MulticastSenderImpl>>> {
        self.forward_tracks.get(user_id).map(|entry| entry.clone())
    }

    /// Get all forward track user IDs
    pub fn get_forward_track_users(&self) -> Vec<String> {
        self.forward_tracks.iter().map(|entry| entry.key().clone()).collect()
    }
}
