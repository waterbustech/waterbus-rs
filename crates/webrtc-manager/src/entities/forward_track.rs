use std::sync::Arc;

use tracing::{debug, warn};
use webrtc::{
    Error,
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::{TrackLocalWriter, track_local_static_rtp::TrackLocalStaticRTP},
};

use super::quality::TrackQuality;

#[derive(Debug, Clone)]
pub struct ForwardTrack {
    pub local_track: Arc<TrackLocalStaticRTP>,
    pub requested_quality: TrackQuality,
    pub effective_quality: TrackQuality,
}

impl ForwardTrack {
    pub fn new(codec: RTCRtpCodecCapability, track_id: String, sid: String) -> Self {
        let local_track = Arc::new(TrackLocalStaticRTP::new(codec, track_id, sid));

        Self {
            local_track,
            requested_quality: TrackQuality::Medium,
            effective_quality: TrackQuality::Medium,
        }
    }

    pub fn set_requested_quality(&mut self, quality: &TrackQuality) {
        if *quality != self.requested_quality {
            debug!("change requested quality to: {:?}", quality);

            self.requested_quality = quality.clone();
        }
    }

    pub fn set_effective_quality(&mut self, quality: &TrackQuality) {
        if *quality != self.effective_quality {
            debug!("change effective quality to: {:?}", quality);

            self.effective_quality = quality.clone();
        }
    }

    pub async fn write_rtp(
        &self,
        rtp: &webrtc::rtp::packet::Packet,
        is_video: bool,
        track_quality: TrackQuality,
    ) {
        let selected_quality = self
            .requested_quality
            .clone()
            .min(self.effective_quality.clone());

        if !is_video || selected_quality == track_quality {
            // Forward the RTP packet
            if let Err(err) = self.local_track.write_rtp(rtp).await {
                if Error::ErrClosedPipe != err {
                    warn!("[track] output track write_rtp got error: {err} and break");
                } else {
                    warn!("[track] output track write_rtp got error: {err}");
                }
            }
        }
    }
}
