use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use kanal::{AsyncReceiver, ReceiveError, Receiver, Sender};

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

use crate::{
    models::{quality::TrackQuality, rtp_foward_info::RtpForwardInfo},
    utils::multicast_sender::MulticastSender,
};

/// ForwardTrack is a track that forwards RTP packets to the local track
pub struct ForwardTrack<M: MulticastSender> {
    pub local_track: Arc<TrackLocalStaticRTP>,
    pub track_id: String,
    requested_quality: Arc<AtomicU8>,
    effective_quality: Arc<AtomicU8>,
    current_subscribed_quality: Arc<AtomicU8>,
    ssrc: u32,
    keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
    multicast: Arc<M>,
    receiver_id: String,
    quality_change_sender: Sender<TrackQuality>,
}

impl<M: MulticastSender + 'static> ForwardTrack<M> {
    /// Create a new ForwardTrack
    ///
    /// # Arguments
    ///
    /// * `codec` - The codec of the track
    /// * `track_id` - The id of the track
    pub fn new(
        codec: RTCRtpCodecCapability,
        track_id: String,
        sid: String,
        forward_track_id: String,
        ssrc: u32,
        keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
        multicast: Arc<M>,
    ) -> Arc<Self> {
        let (quality_change_sender, quality_change_receiver) = kanal::bounded(0);

        // Start with medium quality
        let initial_quality = TrackQuality::Medium;

        let this = Arc::new(Self {
            local_track: Arc::new(TrackLocalStaticRTP::new(codec, track_id.clone(), sid)),
            track_id: forward_track_id.clone(),
            requested_quality: Arc::new(AtomicU8::new(initial_quality.as_u8())),
            effective_quality: Arc::new(AtomicU8::new(initial_quality.as_u8())),
            current_subscribed_quality: Arc::new(AtomicU8::new(initial_quality.as_u8())),
            ssrc,
            keyframe_request_callback,
            multicast,
            receiver_id: forward_track_id,
            quality_change_sender,
        });

        // Subscribe to initial quality layer
        let initial_receiver = this
            .multicast
            .add_receiver_for_quality(initial_quality.clone(), this.receiver_id.clone());

        Self::dynamic_receive_rtp(Arc::clone(&this), initial_receiver, quality_change_receiver);

        this
    }

    /// Set the requested quality
    ///
    /// # Arguments
    ///
    /// * `quality` - The quality to set
    ///
    #[inline]
    pub fn set_requested_quality(&self, quality: &TrackQuality) {
        return;
        let current = TrackQuality::from_u8(self.requested_quality.load(Ordering::Relaxed));
        if *quality != current {
            debug!("[quality] change requested quality to: {:?}", quality);
            self.requested_quality
                .store(quality.as_u8(), Ordering::SeqCst);

            // Notify quality change
            let new_desired = self.get_desired_quality();
            if let Err(_) = self.quality_change_sender.send(new_desired) {
                warn!("[quality] failed to send quality change notification");
            }

            // Request keyframe on quality switch
            if let Some(cb) = &self.keyframe_request_callback {
                cb(self.ssrc);
            }
        }
    }

    /// Set the effective quality
    ///
    /// # Arguments
    ///
    /// * `quality` - The quality to set
    ///
    #[inline]
    pub fn set_effective_quality(&self, quality: &TrackQuality) {
        return;
        let current = TrackQuality::from_u8(self.effective_quality.load(Ordering::Relaxed));
        if *quality != current {
            debug!("[quality] change effective quality to: {:?}", quality);
            self.effective_quality
                .store(quality.as_u8(), Ordering::SeqCst);

            // Notify quality change
            let new_desired = self.get_desired_quality();
            if let Err(_) = self.quality_change_sender.send(new_desired) {
                warn!("[quality] failed to send quality change notification");
            }
        }
    }

    /// Get the desired quality (minimum of requested and effective)
    #[inline]
    pub fn get_desired_quality(&self) -> TrackQuality {
        let requested = TrackQuality::from_u8(self.requested_quality.load(Ordering::Relaxed));
        let effective = TrackQuality::from_u8(self.effective_quality.load(Ordering::Relaxed));
        requested.min(effective)
    }

    /// Switch the subscription to a new quality channel if needed
    ///
    /// # Arguments
    ///
    /// * `new_quality` - The new desired quality
    /// * `this` - Arc<Self> for ForwardTrack
    /// * `receiver_id` - The receiver id
    /// * `multicast` - The multicast sender
    ///
    fn switch_subscription(
        this: &Arc<Self>,
        new_quality: TrackQuality,
    ) -> AsyncReceiver<RtpForwardInfo> {
        let old_quality =
            TrackQuality::from_u8(this.current_subscribed_quality.load(Ordering::Relaxed));
        if old_quality != new_quality {
            // Remove old receiver
            this.multicast
                .remove_receiver_for_quality(old_quality, &this.receiver_id);
            // Add new receiver
            let new_receiver = this
                .multicast
                .add_receiver_for_quality(new_quality, this.receiver_id.clone());
            this.current_subscribed_quality
                .store(new_quality.as_u8(), Ordering::SeqCst);
            debug!(
                "[quality] switched subscription from {:?} to {:?}",
                old_quality, new_quality
            );
            new_receiver
        } else {
            // Already subscribed to the correct quality, just return a dummy receiver (will not be used)
            this.multicast
                .add_receiver_for_quality(new_quality, format!("{}_dummy", this.receiver_id))
        }
    }

    /// Dynamic receive RTP with fast, performant channel switching based on desired quality
    ///
    /// # Arguments
    ///
    /// * `this` - The ForwardTrack
    /// * `initial_receiver` - The initial receiver
    /// * `quality_change_receiver` - Receiver for quality change notifications
    ///
    fn dynamic_receive_rtp(
        this: Arc<Self>,
        mut receiver: AsyncReceiver<RtpForwardInfo>,
        quality_change_receiver: Receiver<TrackQuality>,
    ) {
        tokio::spawn(async move {
            loop {
                match quality_change_receiver.try_recv() {
                    Ok(Some(new_quality)) => {
                        let new_receiver = Self::switch_subscription(&this, new_quality);
                        receiver = new_receiver;
                    }
                    Ok(None) => {}
                    Err(ReceiveError::Closed) | Err(ReceiveError::SendClosed) => {
                        break;
                    }
                }

                match receiver.recv().await {
                    Ok(info) => {
                        let local_track = Arc::clone(&this.local_track);
                        tokio::spawn(async move {
                            Self::process_packet(local_track, info).await;
                        });
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });
    }

    /// Process a batch of RTP packets
    ///
    /// # Arguments
    ///
    /// * `this` - The ForwardTrack
    /// * `batch` - The batch of RTP packets
    ///
    ///
    async fn process_packet(local_track: Arc<TrackLocalStaticRTP>, info: RtpForwardInfo) {
        let is_svc = info.is_svc;

        let should_forward = match is_svc {
            false => true,
            true => {
                let mut vp9_packet = Vp9Packet::default();
                match vp9_packet.depacketize(&info.packet.payload) {
                    Ok(_) => info.track_quality.should_forward_vp9_svc(&vp9_packet),
                    Err(err) => {
                        warn!("Failed to depacketize VP9: {}", err);
                        true
                    }
                }
            }
        };

        if should_forward {
            Self::_write_rtp(&local_track, &info.packet).await;
        }
    }

    /// Write RTP packet
    ///
    /// # Arguments
    ///
    /// * `local_track` - The local track
    /// * `rtp` - The RTP packet
    ///
    #[inline]
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
}
