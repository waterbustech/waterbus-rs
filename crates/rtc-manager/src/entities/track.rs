use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Instant,
};

use dashmap::DashMap;
use str0m::media::{MediaKind, Mid};
use bytes::Bytes;

use crate::{
    models::{
        quality::TrackQuality,
        rtp_forward_info::RtpForwardInfo,
    },
    errors::RtcError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecType {
    VP8,
    VP9,
    H264,
    AV1,
    Opus,
    Other,
}

impl CodecType {
    pub fn from_mime_type(mime_type: &str) -> Self {
        match mime_type.to_lowercase().as_str() {
            s if s.contains("vp8") => CodecType::VP8,
            s if s.contains("vp9") => CodecType::VP9,
            s if s.contains("h264") => CodecType::H264,
            s if s.contains("av1") => CodecType::AV1,
            s if s.contains("opus") => CodecType::Opus,
            _ => CodecType::Other,
        }
    }
}

pub struct Track {
    pub id: String,
    pub room_id: String,
    pub participant_id: String,
    pub mid: Mid,
    pub kind: MediaKind,
    pub codec_type: CodecType,
    pub stream_id: String,
    pub is_simulcast: Arc<AtomicBool>,
    pub is_svc: bool,
    pub ssrc: Arc<AtomicU32>,
    pub quality: TrackQuality,
    pub subscribers: Arc<DashMap<String, TrackQuality>>, // subscriber_id -> requested_quality
    pub last_keyframe_request: Arc<parking_lot::RwLock<Option<Instant>>>,
    pub keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
}

impl Track {
    pub fn new(
        id: String,
        room_id: String,
        participant_id: String,
        mid: Mid,
        kind: MediaKind,
        codec_type: CodecType,
        stream_id: String,
        quality: TrackQuality,
        keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
    ) -> Self {
        let is_svc = matches!(codec_type, CodecType::VP9);
        
        Self {
            id,
            room_id,
            participant_id,
            mid,
            kind,
            codec_type,
            stream_id,
            is_simulcast: Arc::new(AtomicBool::new(false)),
            is_svc,
            ssrc: Arc::new(AtomicU32::new(0)),
            quality,
            subscribers: Arc::new(DashMap::new()),
            last_keyframe_request: Arc::new(parking_lot::RwLock::new(None)),
            keyframe_request_callback,
        }
    }

    pub fn set_ssrc(&self, ssrc: u32) {
        self.ssrc.store(ssrc, Ordering::Relaxed);
    }

    pub fn get_ssrc(&self) -> u32 {
        self.ssrc.load(Ordering::Relaxed)
    }

    pub fn add_subscriber(&self, subscriber_id: String, requested_quality: TrackQuality) {
        self.subscribers.insert(subscriber_id.clone(), requested_quality);
        tracing::debug!("Track {} added subscriber {} with quality {:?}",
                       self.id, subscriber_id, requested_quality);
    }

    pub fn remove_subscriber(&self, subscriber_id: &str) {
        self.subscribers.remove(subscriber_id);
        tracing::debug!("Track {} removed subscriber {}", self.id, subscriber_id);
    }

    pub fn update_subscriber_quality(&self, subscriber_id: &str, quality: TrackQuality) {
        if let Some(mut entry) = self.subscribers.get_mut(subscriber_id) {
            *entry = quality;
            tracing::debug!("Track {} updated subscriber {} quality to {:?}", 
                           self.id, subscriber_id, quality);
        }
    }

    pub fn get_subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    pub fn get_subscribers_for_quality(&self, quality: TrackQuality) -> Vec<String> {
        self.subscribers
            .iter()
            .filter(|entry| *entry.value() == quality)
            .map(|entry| entry.key().clone())
            .collect()
    }

    pub fn set_simulcast(&self, enabled: bool) {
        self.is_simulcast.store(enabled, Ordering::Relaxed);
    }

    pub fn is_simulcast_enabled(&self) -> bool {
        self.is_simulcast.load(Ordering::Relaxed)
    }

    pub fn request_keyframe(&self) -> Result<(), RtcError> {
        let now = Instant::now();
        let mut last_request = self.last_keyframe_request.write();
        
        // Rate limit keyframe requests (minimum 1 second between requests)
        if let Some(last) = *last_request {
            if now.duration_since(last).as_secs() < 1 {
                return Ok(());
            }
        }
        
        *last_request = Some(now);
        
        if let Some(callback) = &self.keyframe_request_callback {
            let ssrc = self.get_ssrc();
            (callback)(ssrc);
        }
        
        tracing::debug!("Keyframe requested for track {}", self.id);
        Ok(())
    }

    pub fn forward_rtp_packet(&self, packet_data: Bytes) -> Result<(), RtcError> {
        // TODO: Parse RTP packet and forward to appropriate subscribers
        // This is where the track forwards data to subscribers based on their quality preferences

        let _rtp_info = RtpForwardInfo::new(
            packet_data,
            self.get_ssrc(),
            0, // TODO: Extract sequence number from RTP packet
            0, // TODO: Extract payload type from RTP packet
            false, // TODO: Extract marker bit from RTP packet
        );

        // Forward to subscribers based on their quality preferences
        for subscriber_entry in self.subscribers.iter() {
            let subscriber_id = subscriber_entry.key();
            let requested_quality = *subscriber_entry.value();
            
            // TODO: Filter and forward based on quality
            // For now, forward to all subscribers
            tracing::debug!("Forwarding RTP packet to subscriber {} (quality: {:?})", 
                           subscriber_id, requested_quality);
        }

        Ok(())
    }

    pub fn get_quality_stats(&self) -> std::collections::HashMap<TrackQuality, usize> {
        let mut stats = std::collections::HashMap::new();
        
        for entry in self.subscribers.iter() {
            let quality = *entry.value();
            *stats.entry(quality).or_insert(0) += 1;
        }
        
        stats
    }

    pub fn should_send_for_quality(&self, quality: TrackQuality) -> bool {
        // Check if any subscriber wants this quality
        self.subscribers.iter().any(|entry| *entry.value() == quality)
    }

    pub fn get_best_available_quality(&self) -> TrackQuality {
        // Return the highest quality requested by any subscriber
        self.subscribers
            .iter()
            .map(|entry| *entry.value())
            .max()
            .unwrap_or(TrackQuality::Medium)
    }

    pub fn stop(&self) {
        self.subscribers.clear();
        tracing::debug!("Track {} stopped", self.id);
    }
}
