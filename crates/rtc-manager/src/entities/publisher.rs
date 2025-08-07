use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use dashmap::DashMap;
use parking_lot::RwLock;
use str0m::{
    change::SdpOffer, media::{Direction, MediaKind}, Candidate, Event, IceConnectionState, Input, Output, Rtc
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    errors::RtcError,
    models::{
        connection_type::ConnectionType,
        data_channel_msg::TrackSubscribedMessage,
        params::{IceCandidateCallback, JoinedCallback},
        streaming_protocol::StreamingProtocol,
    },
};

use super::{subscriber::Subscriber, track::Track};

pub struct Publisher {
    pub participant_id: String,
    pub room_id: String,
    pub rtc: Arc<RwLock<Rtc>>,
    pub connection_type: AtomicU8,
    pub cancel_token: CancellationToken,
    pub subscribers: Arc<DashMap<String, Arc<Subscriber>>>,
    pub tracks: Arc<DashMap<String, Arc<Track>>>,
    pub is_video_enabled: Arc<AtomicU8>,
    pub is_audio_enabled: Arc<AtomicU8>,
    pub is_e2ee_enabled: Arc<AtomicU8>,
    pub streaming_protocol: StreamingProtocol,
    pub track_event_sender: Option<mpsc::UnboundedSender<TrackSubscribedMessage>>,
    pub track_event_receiver: Option<mpsc::UnboundedReceiver<TrackSubscribedMessage>>,
    pub ice_candidate_callback: Option<IceCandidateCallback>,
    pub joined_callback: Option<JoinedCallback>,
}

impl Publisher {
    pub async fn new(
        participant_id: String,
        room_id: String,
        connection_type: ConnectionType,
        is_video_enabled: bool,
        is_audio_enabled: bool,
        is_e2ee_enabled: bool,
        streaming_protocol: StreamingProtocol,
        ice_candidate_callback: IceCandidateCallback,
        joined_callback: JoinedCallback,
    ) -> Result<Arc<Self>, RtcError> {
        // Create str0m RTC instance
        let rtc = Rtc::builder().build();

        let candidate = Candidate::host(addr, "udp").expect("a host candidate");
        rtc.add_local_candidate(candidate).unwrap();

        let (track_event_sender, track_event_receiver) = mpsc::unbounded_channel();

        let publisher = Arc::new(Self {
            participant_id,
            room_id,
            rtc: Arc::new(RwLock::new(rtc)),
            connection_type: AtomicU8::new(connection_type.into()),
            cancel_token: CancellationToken::new(),
            subscribers: Arc::new(DashMap::new()),
            tracks: Arc::new(DashMap::new()),
            is_video_enabled: Arc::new(AtomicU8::new(if is_video_enabled { 1 } else { 0 })),
            is_audio_enabled: Arc::new(AtomicU8::new(if is_audio_enabled { 1 } else { 0 })),
            is_e2ee_enabled: Arc::new(AtomicU8::new(if is_e2ee_enabled { 1 } else { 0 })),
            streaming_protocol,
            track_event_sender: Some(track_event_sender),
            track_event_receiver: Some(track_event_receiver),
            ice_candidate_callback: Some(ice_candidate_callback),
            joined_callback: Some(joined_callback),
        });

        // Start the RTC event loop
        let publisher_clone = Arc::clone(&publisher);
        tokio::spawn(async move {
            publisher_clone.run_rtc_loop().await;
        });

        Ok(publisher)
    }

    pub fn handle_offer(&self, offer_sdp: String) -> Result<String, RtcError> {
        info!("handle_offer: {}", offer_sdp.len());
        let mut rtc = self.rtc.write();

        // Parse and set remote offer
        let offer: str0m::change::SdpOffer =
            SdpOffer::from_sdp_string(&offer_sdp).map_err(|_| RtcError::FailedToSetSdp)?;

        info!("parsed offer");

        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| RtcError::Str0mError(e))?;

        info!("accepted offer");

        // Convert answer to SDP string
        let answer_sdp = answer.to_sdp_string();

        Ok(answer_sdp)
    }

    pub async fn create_offer(&self) -> Result<String, RtcError> {
        let mut rtc = self.rtc.write();

        // Add media tracks based on enabled flags
        let mut changes = rtc.sdp_api();

        if self.is_video_enabled.load(Ordering::Relaxed) == 1 {
            let _video_mid =
                changes.add_media(MediaKind::Video, Direction::SendRecv, None, None, None);
        }

        if self.is_audio_enabled.load(Ordering::Relaxed) == 1 {
            let _audio_mid =
                changes.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
        }

        let (offer, _pending) = changes.apply().ok_or(RtcError::FailedToCreateOffer)?;

        // Convert offer to SDP string
        let offer_sdp = serde_json::to_string(&offer).map_err(|e| RtcError::JsonError(e))?;

        Ok(offer_sdp)
    }

    pub fn add_subscriber(&self, subscriber_id: String, subscriber: Arc<Subscriber>) {
        self.subscribers.insert(subscriber_id, subscriber);

        // Notify subscriber about available tracks
        for track_entry in self.tracks.iter() {
            let _track = track_entry.value();
            // TODO: Send track information to subscriber
        }
    }

    pub fn remove_subscriber(&self, subscriber_id: &str) {
        self.subscribers.remove(subscriber_id);
    }

    pub fn get_subscribers(&self) -> Vec<Arc<Subscriber>> {
        self.subscribers
            .iter()
            .map(|entry| Arc::clone(entry.value()))
            .collect()
    }

    pub fn set_video_enabled(&self, enabled: bool) {
        self.is_video_enabled
            .store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
        // TODO: Update RTC session to enable/disable video
    }

    pub fn set_audio_enabled(&self, enabled: bool) {
        self.is_audio_enabled
            .store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
        // TODO: Update RTC session to enable/disable audio
    }

    pub fn set_e2ee_enabled(&self, enabled: bool) {
        self.is_e2ee_enabled
            .store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
    }

    pub fn close(&self) {
        self.cancel_token.cancel();
        self.subscribers.clear();
        self.tracks.clear();
    }

    async fn run_rtc_loop(&self) {
        let mut last_timeout = Instant::now();

        loop {
            if self.cancel_token.is_cancelled() {
                break;
            }

            let output = {
                let mut rtc = self.rtc.write();
                rtc.poll_output()
            };

            match output {
                Ok(Output::Timeout(timeout)) => {
                    let now = Instant::now();
                    if now >= timeout {
                        last_timeout = now;
                        let mut rtc = self.rtc.write();
                        let _ = rtc.handle_input(Input::Timeout(now));
                    }
                }
                Ok(Output::Transmit(transmit)) => {
                    // TODO: Send data to network
                    tracing::debug!("Need to transmit data: {:?}", transmit.destination);
                }
                Ok(Output::Event(event)) => {
                    self.handle_rtc_event(event).await;
                }
                Err(e) => {
                    tracing::error!("RTC error: {:?}", e);
                    break;
                }
            }

            // Small delay to prevent busy loop
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    async fn handle_rtc_event(&self, event: Event) {
        match event {
            Event::Connected => {
                tracing::info!("Publisher {} connected", self.participant_id);
                if let Some(callback) = &self.joined_callback {
                    (callback)(true).await;
                }
            }
            Event::IceConnectionStateChange(state) => {
                tracing::debug!("ICE connection state changed: {:?}", state);
                if matches!(state, IceConnectionState::Disconnected) {
                    tracing::info!("Publisher {} disconnected", self.participant_id);
                }
            }

            Event::MediaAdded(media) => {
                tracing::info!("Media added: {:?}", media);
                // TODO: Create track and notify subscribers
            }
            Event::MediaData(data) => {
                // Forward media data to subscribers
                self.forward_media_to_subscribers(data).await;
            }
            Event::RtpPacket(packet) => {
                // Forward RTP packet to subscribers
                tracing::debug!("Received RTP packet: {:?}", packet);
            }
            _ => {
                tracing::debug!("Unhandled RTC event: {:?}", event);
            }
        }
    }

    async fn forward_media_to_subscribers(&self, _media_data: str0m::media::MediaData) {
        // TODO: Implement media forwarding to subscribers
        // This is where the publisher controls its subscribers
        for subscriber_entry in self.subscribers.iter() {
            let _subscriber = subscriber_entry.value();
            // Forward media data to this subscriber
        }
    }
}
