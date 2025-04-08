#![allow(unused)]

use std::sync::Arc;

use tracing::info;
use webrtc::rtp_transceiver::RTCRtpTransceiver;
use webrtc::track::track_remote::TrackRemote;

#[derive(Debug, Clone)]
pub struct Track {
    pub track: Arc<TrackRemote>,
    pub receiver: Arc<RTCRtpTransceiver>,
    pub room_id: String,
    pub participant_id: String,
}

impl Track {
    pub fn new(
        track: Arc<TrackRemote>,
        receiver: Arc<RTCRtpTransceiver>,
        room_id: String,
        participant_id: String,
    ) -> Self {
        let handler = Track {
            track,
            receiver,
            room_id,
            participant_id,
        };

        handler.initialize_rtp_handler();
        handler
    }

    fn initialize_rtp_handler(&self) {
        let track = self.track.clone();
        let receiver = self.receiver.clone();

        // // Handle RTP packets for audio
        // track.on_receive_rtp(Box::new(move |packet| {
        //     if track.kind() == "audio" {
        //         if let Err(e) = Self::transcribe_audio(&packet.payload) {
        //             warn!("Error transcribing audio: {}", e);
        //         }
        //     }
        // }));

        // // Start PLI once for RTP
        // track.on_receive_rtp_once(Box::new(move |rtp| {}));
    }

    fn transcribe_audio(payload: &[u8]) -> Result<(), String> {
        // Transcription logic goes here; since we removed Deepgram, we just log the payload for now.
        info!("Transcribing audio: {:?}", payload);
        Ok(())
    }
}
