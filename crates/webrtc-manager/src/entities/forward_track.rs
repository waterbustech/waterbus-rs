use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use dashmap::DashMap;
use tokio::sync::broadcast::Receiver;
use tracing::{debug, warn};
use webrtc::{
    Error,
    rtp::{
        codecs::vp9::Vp9Packet,
        extension::{HeaderExtension, transport_cc_extension::TransportCcExtension},
        packet::Packet,
        packetizer::Depacketizer,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use super::quality::TrackQuality;

#[derive(Debug, Clone)] 
pub struct RtpForwardInfo {
    pub packet: Arc<Packet>,
    pub acceptable_map: Arc<DashMap<(TrackQuality, TrackQuality), bool>>,
    pub is_svc: bool,
    pub track_quality: TrackQuality,
}

#[derive(Debug)]
pub struct ForwardTrack {
    pub local_track: Arc<TrackLocalStaticRTP>,
    requested_quality: Arc<AtomicU8>,
    effective_quality: Arc<AtomicU8>,
}

impl ForwardTrack {
    pub fn new(
        codec: RTCRtpCodecCapability,
        track_id: String,
        sid: String,
        receiver: Receiver<RtpForwardInfo>,
    ) -> Arc<Self> {
        let this = Arc::new(Self {
            local_track: Arc::new(TrackLocalStaticRTP::new(codec, track_id, sid)),
            requested_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            effective_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
        });

        Self::_receive_rtp(Arc::clone(&this), receiver);

        this
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

    fn _receive_rtp(this: Arc<Self>, mut receiver: Receiver<RtpForwardInfo>) {
        tokio::spawn(async move {
            while let Ok(info) = receiver.recv().await {
                let rtp = &info.packet;
                let is_svc = info.is_svc;
                let current_quality = info.track_quality.clone();
                let acceptable_map = info.acceptable_map.clone();

                let desired_quality = this._get_desired_quality();

                if desired_quality == TrackQuality::None {
                    continue;
                }

                let is_video = true;

                if !is_svc
                    && is_video
                    && !Self::_is_acceptable_track(
                        &acceptable_map,
                        current_quality.clone(),
                        desired_quality.clone(),
                    )
                {
                    continue;
                }

                let should_forward = if is_svc && is_video {
                    let mut vp9_packet = Vp9Packet::default();
                    match vp9_packet.depacketize(&rtp.payload) {
                        Ok(_) => desired_quality.should_forward_vp9_svc(&vp9_packet),
                        Err(err) => {
                            warn!("Failed to depacketize VP9: {}", err);
                            true
                        }
                    }
                } else {
                    true
                };

                if !should_forward {
                    continue;
                }

                let rtp_clone = rtp.clone();
                let local_track = Arc::clone(&this.local_track);
                tokio::spawn(async move {
                    Self::_write_rtp(&local_track, &rtp_clone).await;
                });
            }
        });
    }

    fn _get_desired_quality(&self) -> TrackQuality {
        let requested = TrackQuality::from_u8(self.requested_quality.load(Ordering::Relaxed));
        let effective = TrackQuality::from_u8(self.effective_quality.load(Ordering::Relaxed));
        requested.min(effective)
    }

    async fn _write_rtp(local_track: &Arc<TrackLocalStaticRTP>, rtp: &Packet) {
        if let Err(err) = local_track
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

    fn _is_acceptable_track(
        acceptable_map: &Arc<DashMap<(TrackQuality, TrackQuality), bool>>,
        current: TrackQuality,
        desired: TrackQuality,
    ) -> bool {
        acceptable_map
            .get(&(current, desired))
            .map(|v| *v)
            .unwrap_or(false)
    }
}
