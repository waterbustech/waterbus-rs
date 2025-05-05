use dashmap::DashMap;
use egress_manager::egress::hls_writer::HlsWriter;
use egress_manager::egress::moq_writer::MoQWriter;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::track::track_remote::TrackRemote;
use webrtc::util::Marshal;

use crate::entities::quality::TrackQuality;
use crate::errors::WebRTCError;

use super::forward_track::ForwardTrack;

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
    forward_tracks: Arc<DashMap<String, Arc<RwLock<ForwardTrack>>>>,
    // hls_writer: Option<Arc<HlsWriter>>,
    // moq_writer: Option<Arc<MoQWriter>>,
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

        let handler = Track {
            id: track.id(),
            room_id,
            participant_id,
            is_simulcast: false,
            stream_id: track.stream_id().to_string(),
            capability: track.codec().capability,
            kind,
            remote_tracks: vec![track.clone()],
            forward_tracks: Arc::new(DashMap::new()),
            // hls_writer: hls_writer.clone(),
            // moq_writer: moq_writer.clone(),
        };

        handler._forward_rtp(track, hls_writer, moq_writer, kind);

        handler
    }

    pub fn add_track(&mut self, track: Arc<TrackRemote>) {
        self.remote_tracks.push(track.clone());

        self._forward_rtp(track, None, None, self.kind);

        self.is_simulcast = true;
    }

    pub fn stop(&mut self) {
        self.remote_tracks.clear();
        self.forward_tracks.clear();
    }

    pub fn new_forward_track(&self, id: String) -> Result<Arc<RwLock<ForwardTrack>>, WebRTCError> {
        if self.forward_tracks.contains_key(&id) {
            return Err(WebRTCError::FailedToAddTrack);
        }

        let forward_track = Arc::new(RwLock::new(ForwardTrack::new(
            self.capability.clone(),
            self.id.clone(),
            self.stream_id.clone(),
        )));

        self.forward_tracks.insert(id, forward_track.clone());

        Ok(forward_track)
    }

    pub fn remove_forward_track(&self, id: &str) {
        self.forward_tracks.remove(id);
    }

    pub fn _forward_rtp(
        &self,
        remote_track: Arc<TrackRemote>,
        hls_writer: Option<Arc<HlsWriter>>,
        moq_writer: Option<Arc<MoQWriter>>,
        kind: RTPCodecType,
    ) {
        let forward_tracks = self.forward_tracks.clone();
        let track_quality = TrackQuality::from_str(remote_track.rid());

        tokio::spawn(async move {
            let is_video = kind == RTPCodecType::Video;

            while let Ok((rtp, _)) = remote_track.read_rtp().await {
                // Forward to all ForwardTracks
                for entry in forward_tracks.iter() {
                    let forward_track = entry.value().clone();
                    let track_quality = track_quality.clone();

                    let rtp_clone = rtp.clone();
                    tokio::spawn(async move {
                        forward_track
                            .read()
                            .await
                            .write_rtp(&rtp_clone, is_video, track_quality)
                            .await;
                    });
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
