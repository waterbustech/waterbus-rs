use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use crossbeam::channel::{Receiver, Sender, TryRecvError, unbounded};
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
use crate::utils::multicast_sender::MulticastSender;

/// ForwardTrack is a track that forwards RTP packets to the local track
pub struct ForwardTrack {
    pub local_track: Arc<TrackLocalStaticRTP>,
    pub track_id: String,
    requested_quality: Arc<AtomicU8>,
    effective_quality: Arc<AtomicU8>,
    ssrc: u32,
    keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
    multicast: Arc<MulticastSender>,
    receiver_id: String,
    quality_change_sender: Sender<TrackQuality>,
}

impl ForwardTrack {
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
        initial_receiver: Receiver<RtpForwardInfo>,
        forward_track_id: String,
        ssrc: u32,
        keyframe_request_callback: Option<Arc<dyn Fn(u32) + Send + Sync>>,
        multicast: Arc<MulticastSender>,
    ) -> Arc<Self> {
        let (quality_change_sender, quality_change_receiver) = unbounded();

        let this = Arc::new(Self {
            local_track: Arc::new(TrackLocalStaticRTP::new(codec, track_id.clone(), sid)),
            track_id: forward_track_id.clone(),
            requested_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            effective_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            ssrc,
            keyframe_request_callback,
            multicast,
            receiver_id: forward_track_id,
            quality_change_sender,
        });

        Self::_dynamic_receive_rtp(Arc::clone(&this), initial_receiver, quality_change_receiver);

        this
    }

    /// Set the requested quality
    ///
    /// # Arguments
    ///
    /// * `quality` - The quality to set
    ///
    pub fn set_requested_quality(&self, quality: &TrackQuality) {
        let current = TrackQuality::from_u8(self.requested_quality.load(Ordering::Relaxed));
        if *quality != current {
            debug!("[quality] change requested quality to: {:?}", quality);
            self.requested_quality
                .store(quality.as_u8(), Ordering::SeqCst);

            // Check if desired quality changed and notify the processing task
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
    pub fn set_effective_quality(&self, quality: &TrackQuality) {
        let current = TrackQuality::from_u8(self.effective_quality.load(Ordering::Relaxed));
        if *quality != current {
            debug!("[quality] change effective quality to: {:?}", quality);
            self.effective_quality
                .store(quality.as_u8(), Ordering::SeqCst);

            // Check if desired quality changed and notify the processing task
            let new_desired = self.get_desired_quality();
            if let Err(_) = self.quality_change_sender.send(new_desired) {
                warn!("[quality] failed to send quality change notification");
            }
        }
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
        initial_receiver: Receiver<RtpForwardInfo>,
        quality_change_receiver: Receiver<TrackQuality>,
    ) {
        tokio::spawn(async move {
            let mut current_quality = TrackQuality::Medium;
            let mut receiver = initial_receiver;
            const BATCH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(10);

            loop {
                // Check for quality changes first (non-blocking)
                while let Ok(new_desired_quality) = quality_change_receiver.try_recv() {
                    if new_desired_quality != current_quality
                        && new_desired_quality != TrackQuality::None
                    {
                        debug!(
                            "[quality] switching from {:?} to {:?}",
                            current_quality, new_desired_quality
                        );

                        // Switch receiver
                        this.multicast
                            .remove_receiver_for_quality(current_quality, &this.receiver_id);
                        receiver = this.multicast.add_receiver_for_quality(
                            new_desired_quality.clone(),
                            this.receiver_id.clone(),
                        );
                        current_quality = new_desired_quality;
                    }
                }

                let mut batch = Vec::with_capacity(32);
                let batch_start = tokio::time::Instant::now();

                // Collect a batch of packets or timeout
                while batch.len() < 32 && batch_start.elapsed() < BATCH_TIMEOUT {
                    match tokio::task::spawn_blocking({
                        let receiver = receiver.clone();
                        move || receiver.try_recv()
                    })
                    .await
                    {
                        Ok(Ok(info)) => {
                            batch.push(info);
                        }
                        Ok(Err(TryRecvError::Empty)) => {
                            // No more packets available immediately, break to process what we have
                            if !batch.is_empty() {
                                break;
                            }
                            // If batch is empty, wait a bit for new packets
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        }
                        Ok(Err(TryRecvError::Disconnected)) => {
                            // Process remaining batch and exit
                            if !batch.is_empty() {
                                Self::_process_batch(&this, batch).await;
                            }
                            return;
                        }
                        Err(_) => {
                            // Task error, continue
                            break;
                        }
                    }
                }

                // Process batch if we have packets
                if !batch.is_empty() {
                    Self::_process_batch(&this, batch).await;
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

    /// Get the desired quality
    ///
    /// # Arguments
    ///
    /// * `this` - The ForwardTrack
    ///
    pub fn get_desired_quality(&self) -> TrackQuality {
        let requested = TrackQuality::from_u8(self.requested_quality.load(Ordering::Relaxed));
        let effective = TrackQuality::from_u8(self.effective_quality.load(Ordering::Relaxed));
        requested.min(effective)
    }

    /// Write RTP packet
    ///
    /// # Arguments
    ///
    /// * `local_track` - The local track
    /// * `rtp` - The RTP packet
    ///
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

    /// Check if the track is acceptable
    ///
    /// # Arguments
    ///
    /// * `acceptable_map` - The acceptable map
    /// * `current` - The current quality
    /// * `desired` - The desired quality
    /// * `is_simulcast` - Whether the track is simulcast
    ///
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
