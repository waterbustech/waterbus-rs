use dashmap::DashMap;
use egress_manager::egress::hls_writer::HlsWriter;
use egress_manager::egress::moq_writer::MoQWriter;
use std::sync::Arc;
use tracing::info;
use webrtc::Error;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::track::track_local::TrackLocalWriter;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_remote::TrackRemote;
use webrtc::util::Marshal;

use super::subscriber::PreferredQuality;

#[derive(Debug, Clone)]
pub struct Track {
    pub id: String,
    pub room_id: String,
    pub participant_id: String,
    pub is_simulcast: bool,
    pub stream_id: String,
    pub capability: RTCRtpCodecCapability,
    pub kind: RTPCodecType,
    remote_tracks: Vec<Arc<TrackRemote>>,
    pub local_tracks: DashMap<String, Arc<TrackLocalStaticRTP>>,
    hls_writer: Option<Arc<HlsWriter>>,
    moq_writer: Option<Arc<MoQWriter>>,
}

impl Track {
    pub fn new(
        track: Arc<TrackRemote>,
        room_id: String,
        participant_id: String,
        hls_writer: Option<Arc<HlsWriter>>,
        moq_writer: Option<Arc<MoQWriter>>,
    ) -> Self {
        let mut handler = Track {
            id: track.id(),
            room_id,
            participant_id,
            is_simulcast: false,
            stream_id: track.stream_id().to_string(),
            capability: track.codec().capability,
            kind: track.kind(),
            remote_tracks: vec![track.clone()],
            local_tracks: DashMap::new(),
            hls_writer,
            moq_writer,
        };

        handler._create_local_track(track);
        handler
    }

    pub fn add_track(&mut self, track: Arc<TrackRemote>) {
        self.remote_tracks.push(track.clone());
        self._create_local_track(track);

        self.is_simulcast = true;
    }

    pub async fn get_track_appropriate(
        &self,
        preferred: &PreferredQuality,
    ) -> Option<Arc<TrackLocalStaticRTP>> {
        if !self.is_simulcast {
            // Default to "f" for non-simulcast tracks
            return self.local_tracks.get("f").map(|track| Arc::clone(&track));
        }

        // Map PreferredQuality to a preferred RID
        let preferred_rid = match preferred {
            PreferredQuality::High => "f",
            PreferredQuality::Medium => "h",
            PreferredQuality::Low => "q",
        };

        // Try to find the preferred RID first
        if let Some(track) = self.local_tracks.get(preferred_rid) {
            return Some(Arc::clone(&track));
        }

        // Fallback: Try other qualities in order of preference
        let fallback_order = match preferred {
            PreferredQuality::High => vec!["h", "q"],
            PreferredQuality::Medium => vec!["f", "q"],
            PreferredQuality::Low => vec!["h", "f"],
        };

        for rid in fallback_order {
            if let Some(track) = self.local_tracks.get(rid) {
                return Some(Arc::clone(&track));
            }
        }

        // No matching track found
        None
    }

    pub fn stop(&mut self) {
        self.remote_tracks.clear();
        self.local_tracks.clear();
    }

    fn _create_local_track(&mut self, remote_track: Arc<TrackRemote>) {
        let local_track = if remote_track.rid().is_empty() {
            Arc::new(TrackLocalStaticRTP::new(
                self.capability.clone(),
                remote_track.id(),
                remote_track.stream_id(),
            ))
        } else {
            Arc::new(TrackLocalStaticRTP::new_with_rid(
                self.capability.clone(),
                remote_track.id(),
                remote_track.rid().to_owned(),
                remote_track.stream_id(),
            ))
        };

        let rid = if self.is_simulcast {
            remote_track.rid().to_owned()
        } else {
            // Default "f" for audio and non-simulcast video
            "f".to_string()
        };

        self.local_tracks.insert(rid, local_track.clone());

        Self::_forward_rtp(
            remote_track,
            local_track,
            self.hls_writer.clone(),
            self.moq_writer.clone(),
            self.kind,
        );
    }

    pub fn _forward_rtp(
        remote_track: Arc<TrackRemote>,
        local_track: Arc<TrackLocalStaticRTP>,
        hls_writer: Option<Arc<HlsWriter>>,
        moq_writer: Option<Arc<MoQWriter>>,
        kind: RTPCodecType,
    ) {
        tokio::spawn(async move {
            let is_video = kind == RTPCodecType::Video;

            while let Ok((rtp, _)) = remote_track.read_rtp().await {
                // Write RTP to local track
                if let Err(err) = local_track.write_rtp(&rtp).await {
                    if Error::ErrClosedPipe != err {
                        info!("[track] output track write_rtp got error: {err} and break");
                        break;
                    } else {
                        info!("[track] output track write_rtp got error: {err}");
                    }
                }

                // If HLS writer exists, forward the RTP packet for HLS
                if let Some(writer) = &hls_writer {
                    let mut rtp_packet_data = Vec::new();
                    rtp_packet_data.extend_from_slice(&rtp.header.marshal().unwrap());
                    rtp_packet_data.extend_from_slice(rtp.payload.as_ref());

                    let _ = writer.write_rtp(&rtp_packet_data, is_video);
                }

                // If MoQ writer exists, forward the RTP packet for MoQ
                if let Some(writer) = &moq_writer {
                    let mut rtp_packet_data = Vec::new();
                    rtp_packet_data.extend_from_slice(&rtp.header.marshal().unwrap());
                    rtp_packet_data.extend_from_slice(rtp.payload.as_ref());

                    let _ = writer.write_rtp(&rtp_packet_data, is_video);
                }
            }

            info!("[track] exit track loop {}", remote_track.rid());
        });
    }
}
