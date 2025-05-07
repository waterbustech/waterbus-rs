use dashmap::DashMap;
use egress_manager::egress::hls_writer::HlsWriter;
use egress_manager::egress::moq_writer::MoQWriter;
use std::collections::HashMap;
use std::sync::Arc;
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
    forward_tracks: Arc<DashMap<String, Arc<ForwardTrack>>>,
    acceptable_map: Arc<DashMap<(TrackQuality, TrackQuality), bool>>,
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
            acceptable_map: Arc::new(DashMap::new()),
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

    pub fn new_forward_track(&self, id: String) -> Result<Arc<ForwardTrack>, WebRTCError> {
        if self.forward_tracks.contains_key(&id) {
            return Err(WebRTCError::FailedToAddTrack);
        }

        let forward_track = Arc::new(ForwardTrack::new(
            self.capability.clone(),
            self.id.clone(),
            self.stream_id.clone(),
        ));

        self.forward_tracks.insert(id, forward_track.clone());

        Ok(forward_track)
    }

    pub fn remove_forward_track(&self, id: &str) {
        self.forward_tracks.remove(id);
    }

    pub fn rebuild_acceptable_map(&self) {
        let mut quality_map: HashMap<TrackQuality, Arc<TrackRemote>> = HashMap::new();
        for track in &self.remote_tracks {
            let q = TrackQuality::from_str(track.rid());
            quality_map.insert(q, track.clone());
        }

        self.acceptable_map.clear();

        for current in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
            for desired in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
                let result = {
                    let resolved = match desired {
                        TrackQuality::Low => quality_map
                            .get(&TrackQuality::Low)
                            .or(quality_map.get(&TrackQuality::Medium))
                            .or(quality_map.get(&TrackQuality::High)),
                        TrackQuality::Medium => quality_map
                            .get(&TrackQuality::Medium)
                            .or(quality_map.get(&TrackQuality::Low))
                            .or(quality_map.get(&TrackQuality::High)),
                        TrackQuality::High => quality_map
                            .get(&TrackQuality::High)
                            .or(quality_map.get(&TrackQuality::Medium))
                            .or(quality_map.get(&TrackQuality::Low)),
                        TrackQuality::None => None,
                    };
                    resolved.is_some()
                        && matches!(
                            (current, desired),
                            (a, b) if a == b
                                || (a == &TrackQuality::Low && b != &TrackQuality::None)
                                || (a == &TrackQuality::Medium && b != &TrackQuality::None)
                                || (a == &TrackQuality::High && b != &TrackQuality::None)
                        )
                };
                self.acceptable_map
                    .insert((current.clone(), desired.clone()), result);
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
        let forward_tracks = self.forward_tracks.clone();
        let current_quality = TrackQuality::from_str(remote_track.rid());
        let this = self.clone();

        tokio::spawn(async move {
            let is_video = kind == RTPCodecType::Video;

            while let Ok((rtp, _)) = remote_track.read_rtp().await {
                // Forward to all ForwardTracks
                for entry in forward_tracks.iter() {
                    let forward_track = entry.value().clone();
                    let desired_quality = forward_track.get_desired_quality();

                    if !this._is_acceptable_track(current_quality.clone(), desired_quality) {
                        continue;
                    }

                    let rtp_clone = rtp.clone();
                    // Spawn a new task for each forward track to avoid blocking
                    tokio::spawn(async move {
                        forward_track.write_rtp(&rtp_clone).await;
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

    pub fn _is_acceptable_track(&self, current: TrackQuality, desired: TrackQuality) -> bool {
        if !self.is_simulcast {
            return true;
        }

        self.acceptable_map
            .get(&(current, desired))
            .map(|v| *v)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod track_tests {

    use dashmap::DashMap;
    use std::{sync::Arc, vec};

    use crate::entities::quality::TrackQuality;

    struct TestTrack {
        is_simulcast: bool,
        qualities: Vec<TrackQuality>,
        acceptable_map: Arc<DashMap<(TrackQuality, TrackQuality), bool>>,
    }

    impl TestTrack {
        // Create a test track with specific qualities
        fn new(qualities: Vec<TrackQuality>) -> Self {
            let mut track = TestTrack {
                is_simulcast: qualities.len() > 1,

                qualities,
                acceptable_map: Arc::new(DashMap::new()),
            };

            track.rebuild_acceptable_map();
            track
        }

        // This is a simplified version of the original rebuild_acceptable_map function
        fn rebuild_acceptable_map(&mut self) {
            let mut quality_map: std::collections::HashMap<TrackQuality, TrackQuality> =
                std::collections::HashMap::new();
            for q in &self.qualities {
                quality_map.insert(q.clone(), q.clone());
            }

            self.acceptable_map.clear();

            for current in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
                for desired in &[TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
                    let result = {
                        let resolved = match desired {
                            TrackQuality::Low => quality_map
                                .get(&TrackQuality::Low)
                                .or(quality_map.get(&TrackQuality::Medium))
                                .or(quality_map.get(&TrackQuality::High)),
                            TrackQuality::Medium => quality_map
                                .get(&TrackQuality::Medium)
                                .or(quality_map.get(&TrackQuality::Low))
                                .or(quality_map.get(&TrackQuality::High)),
                            TrackQuality::High => quality_map
                                .get(&TrackQuality::High)
                                .or(quality_map.get(&TrackQuality::Medium))
                                .or(quality_map.get(&TrackQuality::Low)),
                            TrackQuality::None => None,
                        };
                        resolved.is_some()
                            && matches!(
                                (current, desired),
                                (a, b) if a == b
                                    || (a == &TrackQuality::Low && b != &TrackQuality::None)
                                    || (a == &TrackQuality::Medium && b != &TrackQuality::None)
                                    || (a == &TrackQuality::High && b != &TrackQuality::None)
                            )
                    };
                    self.acceptable_map
                        .insert((current.clone(), desired.clone()), result);
                }
            }
        }

        // This is a simplified version of the original _is_acceptable_track function
        fn is_acceptable_track(&self, current: TrackQuality, desired: TrackQuality) -> bool {
            if !self.is_simulcast {
                return true;
            }

            self.acceptable_map
                .get(&(current, desired))
                .map(|v| *v)
                .unwrap_or(false)
        }
    }

    // Helper function to convert RID string to TrackQuality
    fn qualities_from_rids(rids: Vec<&str>) -> Vec<TrackQuality> {
        rids.iter().map(|rid| TrackQuality::from_str(rid)).collect()
    }

    #[test]
    fn test_rebuild_acceptable_map_single_track() {
        // Create a track with only high quality ("f")
        let track = TestTrack::new(qualities_from_rids(vec!["f"]));

        // Verify the acceptable map
        assert!(
            track
                .acceptable_map
                .contains_key(&(TrackQuality::High, TrackQuality::High))
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::High, TrackQuality::High))
                .unwrap()
        );

        // High quality track should be usable for medium and low requests too
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::High, TrackQuality::Medium))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::High, TrackQuality::Low))
                .unwrap()
        );

        // Medium quality requests should use high track
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Medium, TrackQuality::High))
                .unwrap()
        );

        // Low quality requests should use high track
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Low, TrackQuality::High))
                .unwrap()
        );
    }

    #[test]
    fn test_rebuild_acceptable_map_simulcast() {
        // Create a track with all three quality levels: q (low), h (medium), f (high)
        let track = TestTrack::new(qualities_from_rids(vec!["q", "h", "f"]));

        // Each quality level should be usable for its own request
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Low, TrackQuality::Low))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Medium, TrackQuality::Medium))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::High, TrackQuality::High))
                .unwrap()
        );

        // Low track should be usable for all quality requests
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Low, TrackQuality::Low))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Low, TrackQuality::Medium))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Low, TrackQuality::High))
                .unwrap()
        );

        // Medium track should be usable for all quality requests
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Medium, TrackQuality::Low))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Medium, TrackQuality::Medium))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Medium, TrackQuality::High))
                .unwrap()
        );

        // High track should be usable for all quality requests
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::High, TrackQuality::Low))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::High, TrackQuality::Medium))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::High, TrackQuality::High))
                .unwrap()
        );
    }

    #[test]
    fn test_rebuild_acceptable_map_partial_simulcast() {
        // Create a track with only low and high quality levels
        let track = TestTrack::new(qualities_from_rids(vec!["q", "f"]));

        // Each available quality level should be usable for its own request
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Low, TrackQuality::Low))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::High, TrackQuality::High))
                .unwrap()
        );

        // Check medium quality fallback
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Medium, TrackQuality::Medium))
                .unwrap()
        );

        // Check specific fallback paths
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Low, TrackQuality::Medium))
                .unwrap()
        );
        assert!(
            *track
                .acceptable_map
                .get(&(TrackQuality::Medium, TrackQuality::Low))
                .unwrap()
        );
    }

    #[test]
    fn test_is_acceptable_track_non_simulcast() {
        // Create a non-simulcast track
        let mut track = TestTrack::new(qualities_from_rids(vec!["f"]));
        track.is_simulcast = false;

        // Any quality combination should be acceptable for non-simulcast
        assert!(track.is_acceptable_track(TrackQuality::Low, TrackQuality::High));
        assert!(track.is_acceptable_track(TrackQuality::Medium, TrackQuality::Low));
        assert!(track.is_acceptable_track(TrackQuality::High, TrackQuality::Medium));
    }

    #[test]
    fn test_is_acceptable_track_simulcast() {
        // Create a simulcast track with all quality levels
        let track = TestTrack::new(qualities_from_rids(vec!["q", "h", "f"]));

        // Same quality level should always be acceptable
        assert!(track.is_acceptable_track(TrackQuality::Low, TrackQuality::Low));
        assert!(track.is_acceptable_track(TrackQuality::Medium, TrackQuality::Medium));
        assert!(track.is_acceptable_track(TrackQuality::High, TrackQuality::High));

        // Check all quality combinations
        assert!(track.is_acceptable_track(TrackQuality::Low, TrackQuality::Medium));
        assert!(track.is_acceptable_track(TrackQuality::Low, TrackQuality::High));
        assert!(track.is_acceptable_track(TrackQuality::Medium, TrackQuality::Low));
        assert!(track.is_acceptable_track(TrackQuality::Medium, TrackQuality::High));
        assert!(track.is_acceptable_track(TrackQuality::High, TrackQuality::Low));
        assert!(track.is_acceptable_track(TrackQuality::High, TrackQuality::Medium));
    }

    #[test]
    fn test_is_acceptable_track_none_quality() {
        // Create a simulcast track
        let track = TestTrack::new(qualities_from_rids(vec!["q", "h", "f"]));

        // TrackQuality::None should not be acceptable
        assert!(!track.is_acceptable_track(TrackQuality::Low, TrackQuality::None));
        assert!(!track.is_acceptable_track(TrackQuality::Medium, TrackQuality::None));
        assert!(!track.is_acceptable_track(TrackQuality::High, TrackQuality::None));
        assert!(!track.is_acceptable_track(TrackQuality::None, TrackQuality::Low));
    }
}
