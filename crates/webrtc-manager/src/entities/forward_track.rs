use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use crossbeam::channel::{Receiver, TryRecvError};
use dashmap::DashMap;

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

use crate::models::{quality::TrackQuality, rtp_foward_info::RtpForwardInfo};

pub struct ForwardTrack {
    pub local_track: Arc<TrackLocalStaticRTP>,
    pub track_id: String,
    requested_quality: Arc<AtomicU8>,
    effective_quality: Arc<AtomicU8>,
    ssrc: u32,
    keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
}

impl ForwardTrack {
    pub fn new(
        codec: RTCRtpCodecCapability,
        track_id: String,
        sid: String,
        receiver: Receiver<RtpForwardInfo>,
        forward_track_id: String,
        ssrc: u32,
        keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
    ) -> Arc<Self> {
        let this = Arc::new(Self {
            local_track: Arc::new(TrackLocalStaticRTP::new(codec, track_id.clone(), sid)),
            track_id: forward_track_id,
            requested_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            effective_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            ssrc,
            keyframe_request_callback,
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
            // Request keyframe on quality switch
            if let Some(cb) = &self.keyframe_request_callback {
                cb(self.ssrc);
            }
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

    fn _receive_rtp(this: Arc<Self>, receiver: Receiver<RtpForwardInfo>) {
        tokio::spawn(async move {
            // Use blocking receiver in a spawn_blocking to avoid blocking the async runtime
            let this_clone = Arc::clone(&this);
            tokio::task::spawn_blocking(move || {
                // Process packets in batches for better performance
                let mut batch = Vec::with_capacity(32);

                loop {
                    // Try to collect a batch of packets
                    match receiver.recv() {
                        Ok(info) => {
                            batch.push(info);

                            // Try to collect more packets without blocking
                            while batch.len() < 32 {
                                match receiver.try_recv() {
                                    Ok(info) => batch.push(info),
                                    Err(TryRecvError::Empty) => break,
                                    Err(TryRecvError::Disconnected) => return,
                                }
                            }

                            // Process the batch
                            let rt = tokio::runtime::Handle::current();
                            rt.block_on(async {
                                Self::_process_batch(&this_clone, std::mem::take(&mut batch)).await;
                            });
                        }
                        Err(_) => {
                            debug!(
                                "[track] receiver disconnected for track {}",
                                this_clone.track_id
                            );
                            break;
                        }
                    }
                }
            })
            .await
            .unwrap_or_else(|e| {
                warn!("[track] spawn_blocking error: {}", e);
            });
        });
    }

    async fn _process_batch(this: &Arc<Self>, batch: Vec<RtpForwardInfo>) {
        for info in batch {
            let is_svc = info.is_svc;
            let is_simulcast = info.is_simulcast;
            let current_quality = info.track_quality.clone();
            let acceptable_map = info.acceptable_map.clone();

            let desired_quality = this.get_desired_quality();

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
                    is_simulcast,
                )
            {
                continue;
            }

            let should_forward = if is_svc && is_video {
                let mut vp9_packet = Vp9Packet::default();
                match vp9_packet.depacketize(&info.packet.payload) {
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

            // Write RTP packet
            Self::_write_rtp(&this.local_track, &info.packet).await;
        }
    }

    pub fn get_desired_quality(&self) -> TrackQuality {
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
        is_simulcast: bool,
    ) -> bool {
        if !is_simulcast {
            return true;
        }

        acceptable_map
            .get(&(current, desired))
            .map(|v| *v)
            .unwrap_or(false)
    }
}
