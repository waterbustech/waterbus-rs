use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread,
};

use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::mpsc::{self, Receiver};
use str0m::{
    change::{SdpAnswer, SdpPendingOffer},
    media::{Direction, MediaKind, Mid},
    Event, IceConnectionState, Rtc,
};
use tokio_util::sync::CancellationToken;

use crate::{
    errors::RtcError,
    models::{
        callbacks::{IceCandidateHandler, RenegotiationHandler},
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
    // pub ice_handler: Option<I>,
    // pub renegotiation_handler: Option<R>,
    pending: Arc<RwLock<Option<SdpPendingOffer>>>,
}

impl Subscriber {
    pub fn new<I, R>(
        participant_id: String,
        target_id: String,
        ice_handler: I,
        _renegotiation_handler: R,
    ) -> Result<Arc<Self>, RtcError>
    where
        I: IceCandidateHandler,
        R: RenegotiationHandler,
    {
        // Create str0m RTC instance
        let mut rtc = Rtc::builder().build();
        
        // Bind UDP socket for this subscriber
        let host_addr = crate::utils::udp_runtime::select_host_address();
        let socket = std::net::UdpSocket::bind(format!("{host_addr}:0"))
            .map_err(|_| RtcError::FailedToCreateOffer)?;
        let local_addr = socket.local_addr()
            .map_err(|_| RtcError::FailedToCreateOffer)?;
        
        // Add local candidate to RTC instance
        let candidate = str0m::Candidate::host(local_addr, str0m::net::Protocol::Udp)
            .map_err(|_| RtcError::InvalidIceCandidate)?;
        rtc.add_local_candidate(candidate);

        // Create event channel
        let (event_tx, event_rx) = mpsc::sync_channel(1);

        let subscriber = Arc::new(Self {
            participant_id: participant_id.clone(),
            target_id: target_id.clone(),
            rtc: Arc::new(RwLock::new(rtc)),
            cancel_token: CancellationToken::new(),
            preferred_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            network_stats: Arc::new(RwLock::new(NetworkStats::default())),
            tracks: Arc::new(DashMap::new()),
            client_requested_quality: Arc::new(RwLock::new(None)),
            send_video_mid: Arc::new(RwLock::new(None)),
            send_audio_mid: Arc::new(RwLock::new(None)),
            pending: Arc::new(RwLock::new(None)),
        });

        // Start UDP run loop
        let subscriber_clone = Arc::clone(&subscriber);
        let socket_clone = socket.try_clone()
            .map_err(|_| RtcError::FailedToCreateOffer)?;
        thread::spawn(move || {
            subscriber_clone.run_udp_loop(socket_clone, event_tx);
        });

        // Announce local candidate
        if let Some(cb) = Some(ice_handler) {
            let ice = crate::utils::ice_utils::IceUtils::convert_from_str0m_candidate(
                &candidate,
                Some("0".to_string()),
                Some(0),
            );
            cb.handle_candidate(ice);
        }

        // Start event loop
        let subscriber_clone = Arc::clone(&subscriber);
        thread::spawn(move || {
            subscriber_clone.run_event_loop(event_rx);
        });

        Ok(subscriber)
    }

    pub async fn handle_offer(&self, offer_sdp: String) -> Result<String, RtcError> {
        let mut rtc = self.rtc.write();

        // Normalize input to raw SDP
        let raw = crate::utils::sdp_utils::SdpUtils::normalize_offer_sdp(&offer_sdp)?;

        // Parse and set remote offer
        let offer: str0m::change::SdpOffer =
            str0m::change::SdpOffer::from_sdp_string(&raw).map_err(|_| RtcError::FailedToSetSdp)?;

        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| RtcError::Str0mError(e))?;

        // Convert answer to SDP string
        let answer_sdp = answer.to_sdp_string();

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

    fn run_event_loop(self: Arc<Self>, rx: Receiver<Event>) {
        while let Ok(event) = rx.recv() {
            self.handle_rtc_event(event);
            if self.cancel_token.is_cancelled() {
                break;
            }
        }
    }

    fn handle_rtc_event(&self, event: Event) {
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

    pub fn receive_media_data(&self, _data: &[u8]) -> Result<(), RtcError> {
        // TODO: Process incoming media data from publisher
        // This is where the subscriber receives data controlled by the publisher
        Ok(())
    }
}
