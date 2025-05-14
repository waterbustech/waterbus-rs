use dashmap::DashMap;
use egress_manager::egress::hls_writer::HlsWriter;
use egress_manager::egress::moq_writer::MoQWriter;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::Sender;
use tokio_util::bytes::BytesMut;
use tracing::debug;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::track::track_remote::TrackRemote;
use webrtc::util::Marshal;

use crate::entities::quality::TrackQuality;
use crate::errors::WebRTCError;

use super::forward_track::{ForwardTrack, RtpForwardInfo};

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
    pub is_simulcast: bool,
    pub is_svc: bool,
    pub codec_type: CodecType,
    pub stream_id: String,
    pub capability: RTCRtpCodecCapability,
    pub kind: RTPCodecType,
    remote_tracks: Vec<Arc<TrackRemote>>,
    forward_tracks: Arc<DashMap<String, Arc<ForwardTrack>>>,
    acceptable_map: Arc<DashMap<(TrackQuality, TrackQuality), bool>>,
    rtp_tx: Sender<RtpForwardInfo>,
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

        let (rtp_tx, _) = tokio::sync::broadcast::channel(128);

        let handler = Track {
            id: track.id(),
            room_id,
            participant_id,
            is_simulcast: false,
            is_svc,
            codec_type,
            stream_id: track.stream_id().to_string(),
            capability: track.codec().capability,
            kind,
            remote_tracks: vec![track.clone()],
            forward_tracks: Arc::new(DashMap::new()),
            acceptable_map: Arc::new(DashMap::new()),
            rtp_tx,
        };

        handler.rebuild_acceptable_map();

        handler._forward_rtp(track, hls_writer, moq_writer, kind);

        handler
    }

    pub fn add_track(&mut self, track: Arc<TrackRemote>) {
        self.remote_tracks.push(track.clone());

        self.rebuild_acceptable_map();

        self._forward_rtp(track, None, None, self.kind);

        self.is_simulcast = true;
    }

    pub fn stop(&mut self) {
        self.remote_tracks.clear();
        self.forward_tracks.clear();
    }

    pub fn new_forward_track(&self, id: &str) -> Result<Arc<ForwardTrack>, WebRTCError> {
        if self.forward_tracks.contains_key(id) {
            return Err(WebRTCError::FailedToAddTrack);
        }

        let receiver = self.rtp_tx.subscribe();

        let forward_track = ForwardTrack::new(
            self.capability.clone(),
            self.id.clone(),
            self.stream_id.clone(),
            receiver,
        );

        self.forward_tracks
            .insert(id.to_owned(), forward_track.clone());

        Ok(forward_track)
    }

    pub fn remove_forward_track(&self, id: &str) {
        self.forward_tracks.remove(id);
    }

    pub fn rebuild_acceptable_map(&self) {
        let mut quality_map: HashMap<TrackQuality, bool> = HashMap::new();

        for track in &self.remote_tracks {
            let q = TrackQuality::from_str(track.rid());
            quality_map.insert(q, true);
        }

        self.acceptable_map.clear();

        let available_qualities: Vec<TrackQuality> =
            [TrackQuality::Low, TrackQuality::Medium, TrackQuality::High]
                .iter()
                .filter(|q| quality_map.contains_key(q))
                .cloned()
                .collect();

        for current in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
            for desired in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
                let acceptable = match available_qualities.len() {
                    3 => current == desired,
                    1 => true,
                    2 => {
                        if !available_qualities.contains(desired) {
                            // Map missing quality to the nearest available
                            let mapped_desired = match desired {
                                TrackQuality::Medium => {
                                    if available_qualities.contains(&TrackQuality::Low) {
                                        TrackQuality::Low
                                    } else {
                                        TrackQuality::High
                                    }
                                }
                                TrackQuality::Low => {
                                    if available_qualities.contains(&TrackQuality::Medium) {
                                        TrackQuality::Medium
                                    } else {
                                        TrackQuality::High
                                    }
                                }
                                TrackQuality::High => {
                                    if available_qualities.contains(&TrackQuality::Medium) {
                                        TrackQuality::Medium
                                    } else {
                                        TrackQuality::Low
                                    }
                                }
                                TrackQuality::None => TrackQuality::None,
                            };
                            current == &mapped_desired
                        } else {
                            current == desired
                        }
                    }
                    _ => false,
                };

                self.acceptable_map
                    .insert((current.clone(), desired.clone()), acceptable);
            }
        }
    }

    pub fn _forward_rtp(
        &self,
        remote_track: Arc<TrackRemote>,
        hls_writer: Option<Arc<HlsWriter>>,
        moq_writer: Option<Arc<MoQWriter>>,
        kind: RTPCodecType,
    ) {
        let sender = self.rtp_tx.clone();
        let current_quality = Arc::new(TrackQuality::from_str(remote_track.rid()));
        let acceptable_map = Arc::clone(&self.acceptable_map);
        let is_svc = self.is_svc;

        tokio::spawn(async move {
            let is_video = kind == RTPCodecType::Video;

            loop {
                let result =
                    tokio::time::timeout(Duration::from_secs(3), remote_track.read_rtp()).await;

                match result {
                    Ok(Ok((rtp, _))) => {
                        let rtp = Arc::new(rtp);

                        if !rtp.payload.is_empty() {
                            let _ = sender.send(RtpForwardInfo {
                                packet: Arc::clone(&rtp),
                                acceptable_map: acceptable_map.clone(),
                                is_svc: is_svc,
                                track_quality: (*current_quality).clone(),
                            });
                        }

                        if hls_writer.is_some() || moq_writer.is_some() {
                            if let Ok(header_bytes) = rtp.header.marshal() {
                                let mut buf =
                                    BytesMut::with_capacity(header_bytes.len() + rtp.payload.len());
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
                        }
                    }
                    Ok(Err(err)) => {
                        debug!("Failed to read rtp: {}", err);
                        break;
                    }
                    Err(_) => {
                        if !remote_track.rid().is_empty() {
                            debug!(
                                "Timeout read rtp from track with rid: {}",
                                remote_track.rid()
                            );
                        }
                    }
                }
            }

            debug!("[track] exit track loop {}", remote_track.rid());
        });
    }
}
