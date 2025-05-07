use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use tracing::{debug, warn};
use webrtc::{
    Error,
    rtp::extension::{HeaderExtension, transport_cc_extension::TransportCcExtension},
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use super::quality::TrackQuality;

#[derive(Debug, Clone)]
pub struct ForwardTrack {
    pub local_track: Arc<TrackLocalStaticRTP>,
    requested_quality: Arc<AtomicU8>,
    effective_quality: Arc<AtomicU8>,
}

impl ForwardTrack {
    pub fn new(codec: RTCRtpCodecCapability, track_id: String, sid: String) -> Self {
        let local_track = Arc::new(TrackLocalStaticRTP::new(codec, track_id, sid));

        Self {
            local_track,
            requested_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            effective_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
        }
    }

    pub fn set_requested_quality(&self, quality: &TrackQuality) {
        let current = TrackQuality::from_u8(self.requested_quality.load(Ordering::Relaxed));
        if *quality != current {
            debug!("[quality] change requested quality to: {:?}", quality);
            self.requested_quality
                .store(quality.as_u8(), Ordering::SeqCst);
        }
    }

    pub fn set_effective_quality(&self, quality: &TrackQuality) {
        let current = TrackQuality::from_u8(self.effective_quality.load(Ordering::Relaxed));
        if *quality != current {
            debug!("[quality] change effective quality to: {:?}", quality);
            self.effective_quality
                .store(quality.as_u8(), Ordering::SeqCst);
        }
    }

    pub fn get_desired_quality(&self) -> TrackQuality {
        let requested = TrackQuality::from_u8(self.requested_quality.load(Ordering::Relaxed));
        let effective = TrackQuality::from_u8(self.effective_quality.load(Ordering::Relaxed));
        requested.min(effective)
    }

    pub async fn write_rtp(&self, rtp: &webrtc::rtp::packet::Packet) {
        // Forward the RTP packet
        if let Err(err) = self
            .local_track
            .write_rtp_with_extensions(
                rtp,
                &[HeaderExtension::TransportCc(TransportCcExtension::default())],
            )
            .await
        {
            if Error::ErrClosedPipe != err {
                warn!("[track] output track write_rtp got error: {err} and break");
            } else {
                warn!("[track] output track write_rtp got error: {err}");
            }
        }
    }
}
