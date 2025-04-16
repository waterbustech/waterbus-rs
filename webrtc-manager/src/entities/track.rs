#![allow(unused)]

use std::sync::Arc;

use tracing::info;
use webrtc::rtp_transceiver::RTCRtpTransceiver;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;

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
    pub local_tracks: Vec<Arc<TrackLocalStaticRTP>>,
}

impl Track {
    pub fn new(track: Arc<TrackRemote>, room_id: String, participant_id: String) -> Self {
        let mut handler = Track {
            id: track.id(),
            room_id,
            participant_id,
            is_simulcast: false,
            stream_id: track.stream_id().to_string(),
            capability: track.codec().capability,
            kind: track.kind(),
            remote_tracks: vec![track.clone()],
            local_tracks: vec![],
        };

        handler._create_local_track(track);
        handler.initialize_rtp_handler();
        handler
    }

    pub fn add_track(&mut self, track: Arc<TrackRemote>) {
        self.remote_tracks.push(track.clone());
        self._create_local_track(track);

        self.is_simulcast = true;
    }

    fn initialize_rtp_handler(&self) {}

    fn transcribe_audio(payload: &[u8]) -> Result<(), String> {
        info!("Transcribing audio: {:?}", payload);
        Ok(())
    }

    pub async fn get_track_appropriate(
        &self,
        preferred: &PreferredQuality,
    ) -> Option<Arc<TrackLocalStaticRTP>> {
        if !self.is_simulcast {
            return self.local_tracks.first().cloned();
        }

        // Map PreferredQuality to a preferred RID
        let preferred_rid = match preferred {
            PreferredQuality::High => "f",
            PreferredQuality::Medium => "h",
            PreferredQuality::Low => "q",
        };

        // Try to find the preferred RID first
        if let Some(track) = self
            .local_tracks
            .iter()
            .find(|t| t.rid() == Some(preferred_rid))
        {
            return Some(Arc::clone(track));
        }

        // Fallback: Try other qualities in order of preference
        let fallback_order = match preferred {
            PreferredQuality::High => vec!["h", "q"],
            PreferredQuality::Medium => vec!["f", "q"],
            PreferredQuality::Low => vec!["h", "f"],
        };

        for rid in fallback_order {
            if let Some(track) = self.local_tracks.iter().find(|t| t.rid() == Some(rid)) {
                return Some(Arc::clone(track));
            }
        }

        // No matching track found
        None
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

        self.local_tracks.push(local_track.clone());

        Self::_forward_rtp(remote_track, local_track);
    }

    pub fn _forward_rtp(remote_track: Arc<TrackRemote>, local_track: Arc<TrackLocalStaticRTP>) {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = remote_track.read_rtp() => {
                        match result {
                            Ok((rtp_packet, _)) => {
                                if let Err(err) = local_track.write_rtp(&rtp_packet).await {
                                    eprintln!("error writing RTP to local track: {:?}", err);
                                    break;
                                }
                            }
                            Err(err) => {
                                break;
                            }
                        }
                    }
                }
            }
        });
    }
}
