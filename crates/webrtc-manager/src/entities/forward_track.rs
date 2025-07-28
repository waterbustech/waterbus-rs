use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use crossbeam_channel::{Receiver, Sender, TryRecvError};

use tokio::sync::mpsc;
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
    current_subscribed_quality: Arc<AtomicU8>, // Track which layer we're currently subscribed to
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
        let (quality_change_sender, quality_change_receiver) = crossbeam_channel::unbounded();

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

        Self::_dynamic_receive_rtp(Arc::clone(&this), initial_receiver, quality_change_receiver);

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

    /// Dynamic receive RTP
    ///
    /// # Arguments
    ///
    /// * `this` - The ForwardTrack
    /// * `initial_receiver` - The initial receiver
    /// * `quality_change_receiver` - Receiver for quality change notifications
    ///
    fn _dynamic_receive_rtp(
        this: Arc<Self>,
        receiver: Receiver<RtpForwardInfo>,
        _quality_change_receiver: Receiver<TrackQuality>,
    ) {
        tokio::task::spawn_blocking(move || {
            while let Ok(info) = receiver.recv() {
                let local_track = Arc::clone(&this.local_track);
                let desired_quality = this.get_desired_quality();
                tokio::spawn(async move {
                    Self::process_packet(local_track, desired_quality, info).await;
                });
            }
        });

        // let cloned = Arc::clone(&this);
        // tokio::spawn(async move {
        //     while let Some(info) = rx.recv().await {
        //         cloned.process_packet(info).await;
        //     }
        // });
    }

    /// Process a batch of RTP packets
    ///
    /// # Arguments
    ///
    /// * `this` - The ForwardTrack
    /// * `batch` - The batch of RTP packets
    ///
    async fn process_packet(
        local_track: Arc<TrackLocalStaticRTP>,
        desired_quality: TrackQuality,
        info: RtpForwardInfo,
    ) {
        let start = std::time::Instant::now();

        if desired_quality == TrackQuality::None {
            return;
        }

        let is_svc = info.is_svc;
        let packet_quality = info.track_quality;

        if !is_svc && packet_quality != desired_quality {
            return;
        }

        let should_forward = if is_svc {
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

        if should_forward {
            Self::_write_rtp(&local_track, &info.packet).await;
        }
        let end = std::time::Instant::now();
        let duration = end.duration_since(start);
        println!("====> Process packet duration: {:?}", duration);
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
