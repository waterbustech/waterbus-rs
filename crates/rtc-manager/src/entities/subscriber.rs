use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

use dashmap::DashMap;
use parking_lot::RwLock;
use str0m::{
    change::{SdpAnswer, SdpPendingOffer},
    media::{Direction, MediaKind, Mid},
    Event, IceConnectionState, Rtc,
};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio_util::sync::CancellationToken;

use crate::{
    errors::RtcError,
    models::{
        params::{IceCandidateCallback, RenegotiationCallback},
        quality::TrackQuality,
    },
};

use super::track::Track;
use crate::utils::udp_runtime::RtcUdpRuntime;

#[derive(Debug, Default, Clone)]
pub struct NetworkStats {
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub packets_received: u64,
    pub packets_sent: u64,
    pub packets_lost: u32,
    pub jitter: f64,
    pub round_trip_time: f64,
}

pub struct Subscriber {
    pub participant_id: String,
    pub target_id: String, // The publisher this subscriber is subscribing to
    pub rtc: Arc<RwLock<Rtc>>,
    pub cancel_token: CancellationToken,
    pub preferred_quality: Arc<AtomicU8>,
    pub network_stats: Arc<RwLock<NetworkStats>>,
    pub tracks: Arc<DashMap<String, Arc<Track>>>,
    pub client_requested_quality: Arc<RwLock<Option<TrackQuality>>>,
    pub send_video_mid: Arc<RwLock<Option<Mid>>>,
    pub send_audio_mid: Arc<RwLock<Option<Mid>>>,
    pub ice_candidate_callback: Option<IceCandidateCallback>,
    pub renegotiation_callback: Option<RenegotiationCallback>,
    pending: Arc<RwLock<Option<SdpPendingOffer>>>,
}

impl Subscriber {
    pub async fn new(
        participant_id: String,
        target_id: String,
        ice_candidate_callback: IceCandidateCallback,
        renegotiation_callback: RenegotiationCallback,
    ) -> Result<Arc<Self>, RtcError> {
        // Create str0m RTC instance
        let rtc = Rtc::builder().build();

        // Create event channel for runtime -> subscriber
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let subscriber = Arc::new(Self {
            participant_id,
            target_id,
            rtc: Arc::new(RwLock::new(rtc)),
            cancel_token: CancellationToken::new(),
            preferred_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            network_stats: Arc::new(RwLock::new(NetworkStats::default())),
            tracks: Arc::new(DashMap::new()),
            client_requested_quality: Arc::new(RwLock::new(None)),
            send_video_mid: Arc::new(RwLock::new(None)),
            send_audio_mid: Arc::new(RwLock::new(None)),
            ice_candidate_callback: Some(ice_candidate_callback),
            renegotiation_callback: Some(renegotiation_callback),
            pending: Arc::new(RwLock::new(None)),
        });

        // Register with global UDP runtime
        let runtime = RtcUdpRuntime::global();
        runtime
            .register_rtc(
                format!("sub:{}:{}", subscriber.participant_id, subscriber.target_id),
                Arc::clone(&subscriber.rtc),
                event_tx,
            )
            .ok();

        // Announce host candidate to the client via callback
        if let (Some(cb), Some(host)) = (
            subscriber.ice_candidate_callback.as_ref(),
            RtcUdpRuntime::global().host_candidate(),
        ) {
            let ice = crate::utils::ice_utils::IceUtils::convert_from_str0m_candidate(
                &host,
                Some("0".to_string()),
                Some(0),
            );
            (cb)(ice).await;
        }

        // Start the RTC event loop
        let subscriber_clone = Arc::clone(&subscriber);
        tokio::spawn(async move {
            subscriber_clone.run_event_loop(event_rx).await;
        });

        Ok(subscriber)
    }

    pub async fn handle_offer(&self, offer_sdp: String) -> Result<String, RtcError> {
        let mut rtc = self.rtc.write();

        // Parse and set remote offer
        let offer: str0m::change::SdpOffer =
            serde_json::from_str(&offer_sdp).map_err(|e| RtcError::JsonError(e))?;

        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| RtcError::Str0mError(e))?;

        // Convert answer to SDP string
        let answer_sdp = serde_json::to_string(&answer).map_err(|e| RtcError::JsonError(e))?;

        Ok(answer_sdp)
    }

    pub fn create_offer(&self) -> Result<String, RtcError> {
        let mut rtc = self.rtc.write();

        // Add media tracks for sending to the browser
        let mut changes = rtc.sdp_api();

        let video_mid = changes.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
        *self.send_video_mid.write() = Some(video_mid);

        let audio_mid = changes.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        *self.send_audio_mid.write() = Some(audio_mid);

        let (offer, pending) = changes.apply().ok_or(RtcError::FailedToCreateOffer)?;

        *self.pending.write() = Some(pending);

        // Convert offer to SDP string
        // let offer_sdp = serde_json::to_string(&offer).map_err(|e| RtcError::JsonError(e))?;

        Ok(offer.to_sdp_string())
    }

    pub fn handle_answer(&self, answer_sdp: String) -> Result<(), RtcError> {
        let answer =
            SdpAnswer::from_sdp_string(&answer_sdp).map_err(|_| RtcError::FailedToSetSdp)?;

        if let Some(pending) = self.pending.write().take() {
            let mut rtc = self.rtc.write();
            rtc.sdp_api()
                .accept_answer(pending, answer)
                .map_err(|e| RtcError::Str0mError(e))?;
        }

        Ok(())
    }

    pub fn get_send_mid(&self, kind: MediaKind) -> Option<Mid> {
        match kind {
            MediaKind::Video => *self.send_video_mid.read(),
            MediaKind::Audio => *self.send_audio_mid.read(),
        }
    }

    pub fn set_preferred_quality(&self, quality: TrackQuality) {
        self.preferred_quality
            .store(quality.as_u8(), Ordering::Relaxed);
        *self.client_requested_quality.write() = Some(quality);

        // TODO: Request quality change from publisher
        tracing::debug!(
            "Subscriber {} requested quality: {:?}",
            self.participant_id,
            quality
        );
    }

    pub fn get_preferred_quality(&self) -> TrackQuality {
        TrackQuality::from_u8(self.preferred_quality.load(Ordering::Relaxed))
    }

    pub fn get_network_stats(&self) -> NetworkStats {
        self.network_stats.read().clone()
    }

    pub fn update_network_stats(&self, stats: NetworkStats) {
        *self.network_stats.write() = stats;
    }

    pub fn add_track(&self, track_id: String, track: Arc<Track>) {
        self.tracks.insert(track_id, track);
    }

    pub fn remove_track(&self, track_id: &str) {
        self.tracks.remove(track_id);
    }

    pub fn get_tracks(&self) -> Vec<Arc<Track>> {
        self.tracks
            .iter()
            .map(|entry| Arc::clone(entry.value()))
            .collect()
    }

    pub fn close(&self) {
        self.cancel_token.cancel();
        self.tracks.clear();
    }

    async fn run_event_loop(self: Arc<Self>, mut rx: UnboundedReceiver<Event>) {
        while let Some(event) = rx.recv().await {
            self.handle_rtc_event(event).await;
            if self.cancel_token.is_cancelled() {
                break;
            }
        }
    }

    async fn handle_rtc_event(&self, event: Event) {
        match event {
            Event::Connected => {
                tracing::info!(
                    "Subscriber {} connected to {}",
                    self.participant_id,
                    self.target_id
                );
            }
            Event::IceConnectionStateChange(state) => {
                tracing::debug!("ICE connection state changed: {:?}", state);
                if matches!(state, IceConnectionState::Disconnected) {
                    tracing::info!(
                        "Subscriber {} disconnected from {}",
                        self.participant_id,
                        self.target_id
                    );
                }
            }

            Event::MediaAdded(media) => {
                tracing::info!("Media added: {:?}", media);
                // TODO: Handle new media track
            }
            Event::MediaData(data) => {
                // Process received media data
                tracing::debug!("Received media data: {} bytes", data.data.len());
                // TODO: Process and potentially forward media data
            }
            Event::RtpPacket(packet) => {
                // Process received RTP packet
                tracing::debug!("Received RTP packet: {:?}", packet);
            }
            _ => {
                tracing::debug!("Unhandled RTC event: {:?}", event);
            }
        }
    }

    pub async fn receive_media_data(&self, _data: &[u8]) -> Result<(), RtcError> {
        // TODO: Process incoming media data from publisher
        // This is where the subscriber receives data controlled by the publisher
        Ok(())
    }
}
