#![allow(unused)]

use std::sync::Arc;

use tracing::info;
use webrtc::rtp_transceiver::RTCRtpTransceiver;
use webrtc::track::track_remote::TrackRemote;

#[derive(Debug, Clone)]
pub struct Track {
    pub track: Arc<TrackRemote>,
    pub room_id: String,
    pub participant_id: String,
}

impl Track {
    pub fn new(track: Arc<TrackRemote>, room_id: String, participant_id: String) -> Self {
        let handler = Track {
            track,
            room_id,
            participant_id,
        };

        handler.initialize_rtp_handler();
        handler
    }

    fn initialize_rtp_handler(&self) {
        let track = self.track.clone();
    }

    fn transcribe_audio(payload: &[u8]) -> Result<(), String> {
        // Transcription logic goes here; since we removed Deepgram, we just log the payload for now.
        info!("Transcribing audio: {:?}", payload);
        Ok(())
    }
}
