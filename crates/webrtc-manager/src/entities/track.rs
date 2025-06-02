use dashmap::DashMap;
use egress_manager::egress::hls_writer::HlsWriter;
use egress_manager::egress::moq_writer::MoQWriter;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::debug;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::track::track_remote::TrackRemote;

use crate::errors::WebRTCError;
use crate::models::quality::TrackQuality;
use crate::models::rtp_foward_info::RtpForwardInfo;
use crate::utils::multicast_sender::MulticastSender;

use super::forward_track::ForwardTrack;

#[derive(Debug, Clone, PartialEq)]
pub enum CodecType {
    H264,
    VP8,
    VP9,
    AV1,
    Other,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: String,
    pub room_id: String,
    pub participant_id: String,
    pub is_simulcast: Arc<AtomicBool>,
    pub is_svc: bool,
    pub codec_type: CodecType,
    pub stream_id: String,
    pub capability: RTCRtpCodecCapability,
    pub kind: RTPCodecType,
    remote_tracks: Vec<Arc<TrackRemote>>,
    forward_tracks: Arc<DashMap<String, Arc<ForwardTrack>>>,
    acceptable_map: Arc<DashMap<(TrackQuality, TrackQuality), bool>>,
    rtp_multicast: MulticastSender,
}

impl Track {
    pub fn new(
        track: Arc<TrackRemote>,
        room_id: String,
        participant_id: String,
        hls_writer: Option<Arc<HlsWriter>>,
        moq_writer: Option<Arc<MoQWriter>>,
    ) -> Self {
        let kind = track.kind();

        // Determine codec type from mime type
        let codec_type = match track.codec().capability.mime_type.to_lowercase().as_str() {
            s if s.contains("vp8") => CodecType::VP8,
            s if s.contains("vp9") => CodecType::VP9,
            s if s.contains("av1") => CodecType::AV1,
            s if s.contains("h264") => CodecType::H264,
            _ => CodecType::Other,
        };

        // Determine if SVC is used based on codec
        let is_svc = match codec_type {
            CodecType::VP9 => true,
            _ => false,
        };

        let rtp_multicast = MulticastSender::new();

        let handler = Track {
            id: track.id(),
            room_id,
            participant_id,
            is_simulcast: Arc::new(AtomicBool::new(false)),
            is_svc,
            codec_type,
            stream_id: track.stream_id().to_string(),
            capability: track.codec().capability,
            kind,
            remote_tracks: vec![track.clone()],
            forward_tracks: Arc::new(DashMap::new()),
            acceptable_map: Arc::new(DashMap::new()),
            rtp_multicast,
        };

        handler.rebuild_acceptable_map();

        handler._forward_rtp(track, hls_writer, moq_writer, kind);

        handler
    }

    pub fn add_track(&mut self, track: Arc<TrackRemote>) {
        self.remote_tracks.push(track.clone());

        self.rebuild_acceptable_map();

        self._forward_rtp(track, None, None, self.kind);

        self.is_simulcast.store(true, Ordering::Relaxed);
    }

    pub fn stop(&mut self) {
        self.remote_tracks.clear();
        self.forward_tracks.clear();
    }

    pub fn new_forward_track(&self, id: &str) -> Result<Arc<ForwardTrack>, WebRTCError> {
        if self.forward_tracks.contains_key(id) {
            return Err(WebRTCError::FailedToAddTrack);
        }

        let receiver = self.rtp_multicast.add_receiver(id.to_string());

        let forward_track = ForwardTrack::new(
            self.capability.clone(),
            self.id.clone(),
            self.stream_id.clone(),
            receiver,
            id.to_string(),
        );

        self.forward_tracks
            .insert(id.to_owned(), forward_track.clone());

        Ok(forward_track)
    }

    pub fn remove_forward_track(&self, id: &str) {
        self.rtp_multicast.remove_receiver(id);
        self.forward_tracks.remove(id);
    }

    pub fn rebuild_acceptable_map(&self) {
        let available_qualities: Vec<TrackQuality> = self
            .remote_tracks
            .iter()
            .map(|track| TrackQuality::from_str(track.rid()))
            .collect::<std::collections::HashSet<_>>() // Remove duplicates
            .into_iter()
            .collect();

        self.acceptable_map.clear();

        // Pre-calculate quality fallback mapping
        let quality_fallback = |desired: &TrackQuality| -> TrackQuality {
            if available_qualities.contains(desired) {
                return desired.clone();
            }

            // Smart fallback logic
            match desired {
                TrackQuality::High => available_qualities
                    .iter()
                    .find(|&q| matches!(q, TrackQuality::Medium))
                    .or_else(|| {
                        available_qualities
                            .iter()
                            .find(|&q| matches!(q, TrackQuality::Low))
                    })
                    .unwrap_or(&TrackQuality::Low)
                    .clone(),
                TrackQuality::Medium => available_qualities
                    .iter()
                    .find(|&q| matches!(q, TrackQuality::Low))
                    .or_else(|| {
                        available_qualities
                            .iter()
                            .find(|&q| matches!(q, TrackQuality::High))
                    })
                    .unwrap_or(&TrackQuality::Low)
                    .clone(),
                TrackQuality::Low => available_qualities
                    .iter()
                    .find(|&q| matches!(q, TrackQuality::Medium))
                    .or_else(|| {
                        available_qualities
                            .iter()
                            .find(|&q| matches!(q, TrackQuality::High))
                    })
                    .unwrap_or(&TrackQuality::Low)
                    .clone(),
                _ => TrackQuality::Low,
            }
        };

        // Build mapping more efficiently
        for current in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
            for desired in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
                let target_quality = quality_fallback(desired);
                let acceptable = current == &target_quality;

                self.acceptable_map
                    .insert((current.clone(), desired.clone()), acceptable);
            }
        }
    }

    pub fn _forward_rtp(
        &self,
        remote_track: Arc<TrackRemote>,
        _hls_writer: Option<Arc<HlsWriter>>,
        _moq_writer: Option<Arc<MoQWriter>>,
        kind: RTPCodecType,
    ) {
        let multicast = self.rtp_multicast.clone();
        let current_quality = Arc::new(TrackQuality::from_str(remote_track.rid()));
        let acceptable_map = Arc::clone(&self.acceptable_map);
        let is_svc = self.is_svc;
        let is_simulcast = Arc::clone(&self.is_simulcast);

        tokio::spawn(async move {
            let _is_video = kind == RTPCodecType::Video;

            loop {
                let result = remote_track.read_rtp().await;

                match result {
                    Ok((rtp, _)) => {
                        if !rtp.payload.is_empty() {
                            let info = RtpForwardInfo {
                                packet: Arc::new(rtp),
                                acceptable_map: acceptable_map.clone(),
                                is_svc,
                                is_simulcast: is_simulcast.load(Ordering::Relaxed),
                                track_quality: (*current_quality).clone(),
                            };

                            multicast.send(info);
                        }
                    }
                    Err(err) => {
                        debug!("Failed to read RTP: {}", err);
                        break;
                    }
                }
            }

            debug!("[track] exit track loop {}", remote_track.rid());
        });
    }
}
