use std::{
    sync::{Arc, Weak},
    sync::atomic::{AtomicU8, Ordering},
    time::{Duration, Instant},
};

use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use str0m::{
    Rtc, Candidate, Input, Output, Event, IceConnectionState,
    media::{Direction, MediaKind, Mid, KeyframeRequest, KeyframeRequestKind, MediaData},
    change::{SdpOffer, SdpAnswer},
};
use tracing::{warn, info, debug};

use crate::models::{connection_type::ConnectionType, data_channel_msg::TrackSubscribedMessage};

use super::media::Media;

pub struct Publisher {
    pub media: Arc<RwLock<Media>>,
    pub rtc: Arc<Mutex<Rtc>>,
    pub connection_type: AtomicU8,
    pub cancel_token: CancellationToken,
    pub track_event_receiver: Option<mpsc::UnboundedReceiver<TrackSubscribedMessage>>,
}

impl Publisher {
    pub async fn new(
        media: Arc<RwLock<Media>>,
        rtc: Arc<Mutex<Rtc>>,
        connection_type: ConnectionType,
    ) -> Arc<Self> {
        let publisher = Arc::new(Self {
            media,
            rtc,
            connection_type: AtomicU8::new(connection_type.into()),
            cancel_token: CancellationToken::new(),
            track_event_receiver: None,
        });

        let publisher_clone = Arc::clone(&publisher);
        
        // Set up media communication - simplified for str0m
        publisher_clone.setup_media_communication().await;

        publisher
    }

    async fn setup_media_communication(&self) {
        // In str0m, we don't need the complex setup that webrtc required
        // The RTC instance handles most of the complexity internally
        
        // Set up keyframe request callback
        let publisher_weak = Arc::downgrade(&(self as *const Self as *const Publisher));
        let mut media = self.media.write();
        media.keyframe_request_callback = Some(Arc::new(move |ssrc: u32| {
            // In str0m, keyframe requests are handled differently
            // We would need to trigger a keyframe request through the writer
            info!("Keyframe request for SSRC: {}", ssrc);
        }));
    }

    pub async fn handle_renegotiation(&self, offer: SdpOffer) -> Result<SdpAnswer, str0m::RtcError> {
        let mut rtc = self.rtc.lock();
        rtc.accept_offer(offer)
    }

    pub async fn handle_migration(&self, offer: SdpOffer) -> Result<SdpAnswer, str0m::RtcError> {
        // For migration, we might need to recreate the RTC instance or handle it differently
        // For now, treat it the same as renegotiation
        self.handle_renegotiation(offer).await
    }

    pub fn add_remote_candidate(&self, candidate: Candidate) -> Result<(), str0m::RtcError> {
        let mut rtc = self.rtc.lock();
        rtc.add_remote_candidate(candidate);
        Ok(())
    }

    // Keyframe request methods - adapted for str0m
    #[inline]
    pub fn send_rtcp_pli(&self, media_ssrc: u32) {
        // In str0m, keyframe requests are handled through the media writer
        // This is a simplified version
        let rtc = self.rtc.clone();
        let cancel = self.cancel_token.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        break;
                    }
                    _ = interval.tick() => {
                        // In str0m, we would request keyframe through the media writer
                        // This requires knowing the Mid and using the writer
                        info!("Requesting keyframe for SSRC: {}", media_ssrc);
                    }
                }
            }
        });
    }

    #[inline]
    pub fn send_rtcp_pli_once(&self, media_ssrc: u32) {
        // Similar to above but only once
        info!("Requesting keyframe once for SSRC: {}", media_ssrc);
    }

    #[inline]
    pub fn set_connection_type(&self, connection_type: ConnectionType) {
        self.connection_type
            .store(connection_type.into(), Ordering::Relaxed);
    }

    #[inline]
    pub fn get_connection_type(&self) -> ConnectionType {
        self.connection_type.load(Ordering::Relaxed).into()
    }

    #[inline]
    pub fn close(&self) {
        let rtc = self.rtc.clone();
        let media = self.media.clone();
        self.cancel_token.cancel();

        tokio::spawn(async move {
            {
                let mut rtc_guard = rtc.lock();
                rtc_guard.disconnect();
            }
            
            // Stop media
            let media = media.write();
            media.stop();
        });
    }

    // Method to poll the RTC instance and handle events
    pub fn poll_rtc(&self) -> Option<Output> {
        let mut rtc = self.rtc.lock();
        match rtc.poll_output() {
            Ok(output) => Some(output),
            Err(e) => {
                warn!("RTC poll error: {:?}", e);
                None
            }
        }
    }

    // Method to handle input to the RTC instance
    pub fn handle_input(&self, input: Input) -> Result<(), str0m::RtcError> {
        let mut rtc = self.rtc.lock();
        rtc.handle_input(input)
    }

    // Method to get a media writer for sending data
    pub fn get_media_writer(&self, mid: Mid) -> Option<str0m::media::Writer> {
        let mut rtc = self.rtc.lock();
        rtc.writer(mid)
    }

    // Method to request keyframe through str0m
    pub fn request_keyframe(&self, mid: Mid, rid: Option<str0m::media::Rid>) -> Result<(), str0m::RtcError> {
        if let Some(mut writer) = self.get_media_writer(mid) {
            writer.request_keyframe(rid, KeyframeRequestKind::Pli)
        } else {
            Err(str0m::RtcError::InvalidMid(mid))
        }
    }
}
