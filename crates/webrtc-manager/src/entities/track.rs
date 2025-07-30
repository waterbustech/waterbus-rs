use bytes::BytesMut;
use dashmap::DashMap;
use egress_manager::egress::hls_writer::HlsWriter;
use egress_manager::egress::moq_writer::MoQWriter;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::debug;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::track::track_remote::TrackRemote;
use webrtc::util::Marshal;

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

/// Track is a track that is used to forward RTP packets to the local track
/// It is used to forward RTP packets to the local track
#[derive(Clone)]
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
    pub remote_tracks: Vec<Arc<TrackRemote>>,
    pub forward_tracks: Arc<DashMap<String, Arc<ForwardTrack<MulticastSenderImpl>>>>,
    pub ssrc: u32,
    // acceptable_map: Arc<DashMap<(TrackQuality, TrackQuality), bool>>,
    rtp_multicast: Arc<MulticastSenderImpl>,
    keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
}

impl Track {
    /// Create a new Track
    ///
    /// # Arguments
    ///
    /// * `track` - The track to create the Track for
    /// * `room_id` - The id of the room
    pub fn new(
        track: Arc<TrackRemote>,
        room_id: String,
        participant_id: String,
        hls_writer: Option<Arc<HlsWriter>>,
        moq_writer: Option<Arc<MoQWriter>>,
        keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
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
        let is_svc = matches!(codec_type, CodecType::VP9);

        let rtp_multicast = Arc::new(MulticastSenderImpl::new());

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
            // acceptable_map: Arc::new(DashMap::new()),
            ssrc: track.ssrc(),
            rtp_multicast,
            keyframe_request_callback: keyframe_request_callback.clone(),
        };

        // handler.rebuild_acceptable_map();

        handler._forward_rtp(track, hls_writer, moq_writer, kind);

        handler
    }

    /// Add a new track to the Track
    ///
    /// # Arguments
    ///
    /// * `track` - The track to add to the Track
    ///
    #[inline]
    pub fn add_track(&mut self, track: Arc<TrackRemote>) {
        self.remote_tracks.push(track.clone());

        // self.rebuild_acceptable_map();

        self._forward_rtp(track, None, None, self.kind);

        self.is_simulcast.store(true, Ordering::Relaxed);
    }

    /// Stop the Track
    ///
    /// # Arguments
    ///
    /// * `track` - The track to stop
    ///
    #[inline]
    pub fn stop(&mut self) {
        self.remote_tracks.clear();
        self.forward_tracks.clear();
        // self.acceptable_map.clear();
        self.is_simulcast.store(false, Ordering::Relaxed);
        self.rtp_multicast.clear();
    }

    /// Create a new ForwardTrack
    ///
    /// # Arguments
    ///
    /// * `id` - The id of the ForwardTrack
    /// * `ssrc` - The ssrc of the ForwardTrack
    ///
    pub fn new_forward_track(
        &self,
        id: &str,
        ssrc: u32,
    ) -> Result<Arc<ForwardTrack<MulticastSenderImpl>>, WebRTCError> {
        if self.forward_tracks.contains_key(id) {
            return Err(WebRTCError::FailedToAddTrack);
        }
        // Start with Medium (h) quality; ForwardTrack will switch as needed

        let forward_track = ForwardTrack::new(
            self.capability.clone(),
            self.id.clone(),
            self.stream_id.clone(),
            // receiver,
            id.to_string(),
            ssrc,
            self.keyframe_request_callback.clone(),
            self.rtp_multicast.clone(), // pass Arc<MulticastSender>
        );
        self.forward_tracks
            .insert(id.to_owned(), forward_track.clone());
        Ok(forward_track)
    }

    /// Remove a ForwardTrack
    ///
    /// # Arguments
    ///
    /// * `id` - The id of the ForwardTrack
    ///
    #[inline]
    pub fn remove_forward_track(&self, id: &str) {
        // Remove from all quality lines
        self.rtp_multicast
            .remove_receiver_for_quality(TrackQuality::High, id);
        self.rtp_multicast
            .remove_receiver_for_quality(TrackQuality::Medium, id);
        self.rtp_multicast
            .remove_receiver_for_quality(TrackQuality::Low, id);
        self.forward_tracks.remove(id);
    }

    /// Rebuild the acceptable map
    ///
    /// # Arguments
    ///
    /// * `self` - The Track
    ///
    // pub fn rebuild_acceptable_map(&self) {
    //     let available_qualities: Vec<TrackQuality> = self
    //         .remote_tracks
    //         .iter()
    //         .map(|track| TrackQuality::from_str(track.rid()).unwrap())
    //         .collect::<std::collections::HashSet<_>>() // Remove duplicates
    //         .into_iter()
    //         .collect();

    //     self.acceptable_map.clear();

    //     // Pre-calculate quality fallback mapping
    //     let quality_fallback = |desired: &TrackQuality| -> TrackQuality {
    //         if available_qualities.contains(desired) {
    //             return desired.clone();
    //         }

    //         // Smart fallback logic
    //         match desired {
    //             TrackQuality::High => available_qualities
    //                 .iter()
    //                 .find(|&q| matches!(q, TrackQuality::Medium))
    //                 .or_else(|| {
    //                     available_qualities
    //                         .iter()
    //                         .find(|&q| matches!(q, TrackQuality::Low))
    //                 })
    //                 .unwrap_or(&TrackQuality::Low)
    //                 .clone(),
    //             TrackQuality::Medium => available_qualities
    //                 .iter()
    //                 .find(|&q| matches!(q, TrackQuality::Low))
    //                 .or_else(|| {
    //                     available_qualities
    //                         .iter()
    //                         .find(|&q| matches!(q, TrackQuality::High))
    //                 })
    //                 .unwrap_or(&TrackQuality::Low)
    //                 .clone(),
    //             TrackQuality::Low => available_qualities
    //                 .iter()
    //                 .find(|&q| matches!(q, TrackQuality::Medium))
    //                 .or_else(|| {
    //                     available_qualities
    //                         .iter()
    //                         .find(|&q| matches!(q, TrackQuality::High))
    //                 })
    //                 .unwrap_or(&TrackQuality::Low)
    //                 .clone(),
    //             _ => TrackQuality::Low,
    //         }
    //     };

    //     // Build mapping more efficiently
    //     for current in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
    //         for desired in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
    //             let target_quality = quality_fallback(desired);
    //             let acceptable = current == &target_quality;

    //             self.acceptable_map
    //                 .insert((current.clone(), desired.clone()), acceptable);
    //         }
    //     }
    // }

    /// Forward RTP
    ///
    /// # Arguments
    ///
    /// * `remote_track` - The remote track to forward
    /// * `hls_writer` - The hls writer to write to the cloud storage
    /// * `moq_writer` - The moq writer to write to the moq host
    /// * `kind` - The kind of the track (video or audio)
    ///
    pub fn _forward_rtp(
        &self,
        remote_track: Arc<TrackRemote>,
        hls_writer: Option<Arc<HlsWriter>>,
        moq_writer: Option<Arc<MoQWriter>>,
        kind: RTPCodecType,
    ) {
        let multicast = self.rtp_multicast.clone();
        let current_quality = Arc::new(TrackQuality::from_str(remote_track.rid()).unwrap());
        let is_svc = self.is_svc;
        let is_simulcast = Arc::clone(&self.is_simulcast);

        tokio::spawn(async move {
            let is_video = kind == RTPCodecType::Video;

            loop {
                let result = remote_track.read_rtp().await;

                match result {
                    Ok((rtp, _)) => {
                        if !rtp.payload.is_empty() {
                            if hls_writer.is_some() || moq_writer.is_some() {
                                if let Ok(header_bytes) = rtp.header.marshal() {
                                    let mut buf = BytesMut::with_capacity(
                                        header_bytes.len() + rtp.payload.len(),
                                    );
                                    buf.extend_from_slice(&header_bytes);
                                    buf.extend_from_slice(&rtp.payload);

                                    if let Some(writer) = &hls_writer {
                                        let _ = writer.write_rtp(&buf, is_video);
                                    }

                                    if let Some(writer) = &moq_writer {
                                        let _ = writer.write_rtp(&buf, is_video);
                                    }
                                } else {
                                    debug!("Failed to marshal RTP header");
                                }
                            } else {
                                let info = RtpForwardInfo {
                                    packet: Arc::new(rtp),
                                    is_svc,
                                    track_quality: (*current_quality).clone(),
                                };

                                // For simulcast, send to the correct quality line based on rid
                                if is_simulcast.load(Ordering::Relaxed) {
                                    let quality = TrackQuality::from_str(remote_track.rid())
                                        .unwrap_or(TrackQuality::Medium);
                                    multicast.send_to_quality(quality, info);
                                } else {
                                    // Non-simulcast: always use Medium (h)
                                    multicast.send_to_quality(TrackQuality::Medium, info);
                                }
                            }
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
