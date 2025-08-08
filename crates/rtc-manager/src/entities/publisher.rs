use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use dashmap::DashMap;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use str0m::{Rtc, Event, Input, Output, IceConnectionState, media::{Direction, MediaKind}};

use crate::{
    services::udp_socket_manager::UdpSocketManager,
    models::{
        connection_type::ConnectionType,
        data_channel_msg::TrackSubscribedMessage,
        streaming_protocol::StreamingProtocol,
        params::{IceCandidateCallback, JoinedCallback, RtcManagerConfigs},
        quality::TrackQuality,
    },
    errors::RtcError,
    services::quality_manager::{QualityManager, NetworkConditions},
};

use super::{subscriber::Subscriber, track::Track};

#[derive(Debug, Clone)]
pub struct SimulcastLayer {
    pub quality: TrackQuality,
    pub ssrc: u32,
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
    pub bitrate: u32,
    pub is_active: bool,
}

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
    pub quality_manager: Arc<QualityManager>,
    /// Simulcast layers: High, Medium, Low quality streams
    pub simulcast_layers: Arc<DashMap<TrackQuality, SimulcastLayer>>,
    /// Shared UDP socket manager
    pub udp: Arc<RwLock<UdpSocketManager>>,
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
        configs: RtcManagerConfigs,
        udp: Arc<RwLock<UdpSocketManager>>,
    ) -> Result<Arc<Self>, RtcError> {
        // Create str0m RTC instance
        let mut rtc = Rtc::builder().build();

        // Add local candidate from the shared UDP socket (chat.rs style)
        let candidate = udp.read().create_host_candidate()?;
        rtc.add_local_candidate(candidate);

        let (track_event_sender, track_event_receiver) = mpsc::unbounded_channel();

        // Initialize simulcast layers
        let simulcast_layers = Arc::new(DashMap::new());

        // Add default simulcast layers for video
        if is_video_enabled {
            simulcast_layers.insert(TrackQuality::High, SimulcastLayer {
                quality: TrackQuality::High,
                ssrc: 0, // Will be set when media is added
                width: 1920,
                height: 1080,
                framerate: 30,
                bitrate: 2_000_000, // 2 Mbps
                is_active: true,
            });

            simulcast_layers.insert(TrackQuality::Medium, SimulcastLayer {
                quality: TrackQuality::Medium,
                ssrc: 0, // Will be set when media is added
                width: 1280,
                height: 720,
                framerate: 30,
                bitrate: 1_000_000, // 1 Mbps
                is_active: true,
            });

            simulcast_layers.insert(TrackQuality::Low, SimulcastLayer {
                quality: TrackQuality::Low,
                ssrc: 0, // Will be set when media is added
                width: 640,
                height: 360,
                framerate: 15,
                bitrate: 300_000, // 300 Kbps
                is_active: true,
            });
        }

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
            quality_manager: Arc::new(QualityManager::new()),
            simulcast_layers,
            udp,
        });

        // Start the RTC event loop
        let publisher_clone = Arc::clone(&publisher);
        tokio::spawn(async move {
            publisher_clone.run_rtc_loop().await;
        });

        Ok(publisher)
    }

    pub fn handle_offer(&self, offer_sdp: String) -> Result<String, RtcError> {
        tracing::info!("handle_offer: {}", offer_sdp.len());
        let mut rtc = self.rtc.write();

        // Parse and set remote offer
        let offer: str0m::change::SdpOffer =
            str0m::change::SdpOffer::from_sdp_string(&offer_sdp).map_err(|_| RtcError::FailedToSetSdp)?;

        tracing::info!("parsed offer");

        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| RtcError::Str0mError(e))?;

        tracing::info!("accepted offer");

        // Convert answer to SDP string
        let answer_sdp = answer.to_sdp_string();

        Ok(answer_sdp)
    }

    pub fn create_offer(&self) -> Result<String, RtcError> {
        let mut rtc = self.rtc.write();

        // Add media tracks based on enabled flags
        let mut changes = rtc.sdp_api();

        if self.is_video_enabled.load(Ordering::Relaxed) == 1 {
            let _video_mid = changes.add_media(
                MediaKind::Video,
                Direction::SendRecv,
                None,
                None,
                None,
            );
        }

        if self.is_audio_enabled.load(Ordering::Relaxed) == 1 {
            let _audio_mid = changes.add_media(
                MediaKind::Audio,
                Direction::SendRecv,
                None,
                None,
                None,
            );
        }

        let (offer, _pending) = changes.apply()
            .ok_or(RtcError::FailedToCreateOffer)?;

        // Convert offer to SDP string
        let offer_sdp = offer.to_sdp_string();

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
        self.subscribers.iter().map(|entry| Arc::clone(entry.value())).collect()
    }

    pub fn set_video_enabled(&self, enabled: bool) {
        self.is_video_enabled.store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
        // TODO: Update RTC session to enable/disable video
    }

    pub fn set_audio_enabled(&self, enabled: bool) {
        self.is_audio_enabled.store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
        // TODO: Update RTC session to enable/disable audio
    }

    pub fn set_e2ee_enabled(&self, enabled: bool) {
        self.is_e2ee_enabled.store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
    }

    pub fn close(&self) {
        self.cancel_token.cancel();
        self.subscribers.clear();
        self.tracks.clear();
    }

    /// Update network conditions for quality adaptation
    pub fn update_network_conditions(&self, conditions: NetworkConditions) {
        self.quality_manager.update_network_conditions(conditions);

        // Check if quality change is needed
        if let Some(recommendation) = self.quality_manager.get_quality_recommendation() {
            tracing::info!("Quality change recommended: {:?} -> {:?} (reason: {:?})",
                         recommendation.from_quality, recommendation.to_quality, recommendation.reason);

            // Apply quality change to all subscribers
            self.apply_quality_change_to_subscribers(recommendation.to_quality);
        }
    }

    /// Apply quality change to all subscribers
    fn apply_quality_change_to_subscribers(&self, target_quality: TrackQuality) {
        for subscriber_entry in self.subscribers.iter() {
            let subscriber = subscriber_entry.value();
            subscriber.set_preferred_quality(target_quality);
        }
    }

    /// Get available simulcast layers
    pub fn get_simulcast_layers(&self) -> Vec<SimulcastLayer> {
        self.simulcast_layers.iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Enable/disable a specific simulcast layer
    pub fn set_simulcast_layer_active(&self, quality: TrackQuality, active: bool) {
        if let Some(mut layer) = self.simulcast_layers.get_mut(&quality) {
            layer.is_active = active;
            tracing::info!("Simulcast layer {:?} set to active: {}", quality, active);
        }
    }

    /// Get the best available simulcast layer for a subscriber
    pub fn get_best_layer_for_subscriber(&self, subscriber_quality: TrackQuality) -> Option<SimulcastLayer> {
        // Try to find the requested quality first
        if let Some(layer) = self.simulcast_layers.get(&subscriber_quality) {
            if layer.is_active {
                return Some(layer.clone());
            }
        }

        // Fall back to lower quality if requested is not available
        let fallback_order = match subscriber_quality {
            TrackQuality::High => vec![TrackQuality::Medium, TrackQuality::Low],
            TrackQuality::Medium => vec![TrackQuality::Low, TrackQuality::High],
            TrackQuality::Low => vec![TrackQuality::Medium, TrackQuality::High],
        };

        for quality in fallback_order {
            if let Some(layer) = self.simulcast_layers.get(&quality) {
                if layer.is_active {
                    return Some(layer.clone());
                }
            }
        }

        None
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
                    // Send data to network via shared UDP socket
                    if let Err(e) = self.udp.read().send_transmit(transmit) {
                        tracing::error!("Failed to send transmit: {}", e);
                    }
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
                self.forward_rtp_to_subscribers(packet).await;
            }
            Event::RtcpPacket(rtcp) => {
                // Handle RTCP feedback for quality adaptation
                self.handle_rtcp_feedback(&rtcp).await;
            }
            _ => {
                tracing::debug!("Unhandled RTC event: {:?}", event);
            }
        }
    }

    async fn forward_media_to_subscribers(&self, media_data: str0m::media::MediaData) {
        // Forward media data to all subscribers based on their preferred quality
        tracing::debug!("Forwarding media data: {} bytes to {} subscribers",
                       media_data.data.len(), self.subscribers.len());

        for subscriber_entry in self.subscribers.iter() {
            let subscriber = subscriber_entry.value();
            let subscriber_quality = subscriber.get_preferred_quality();

            // Get the best available simulcast layer for this subscriber
            if let Some(layer) = self.get_best_layer_for_subscriber(subscriber_quality) {
                tracing::debug!("Forwarding {:?} quality layer to subscriber {}",
                              layer.quality, subscriber.participant_id);

                // Send media data to subscriber's RTC instance
                {
                    let mut subscriber_rtc = subscriber.rtc.write();

                    // Create input for the subscriber's RTC instance
                    let input = str0m::Input::Receive(
                        std::time::Instant::now(),
                        str0m::net::Receive {
                            source: "127.0.0.1:0".parse().unwrap(), // Placeholder source
                            destination: "127.0.0.1:0".parse().unwrap(), // Placeholder destination
                            contents: media_data.data.clone(),
                        }
                    );

                    if let Err(e) = subscriber_rtc.handle_input(input) {
                        tracing::error!("Failed to forward media to subscriber {}: {}",
                                      subscriber.participant_id, e);
                    }
                }
            } else {
                tracing::warn!("No suitable simulcast layer available for subscriber {} (requested: {:?})",
                             subscriber.participant_id, subscriber_quality);
            }
        }
    }

    async fn forward_rtp_to_subscribers(&self, rtp_packet: str0m::rtp::RtpPacket) {
        // Forward RTP packet to subscribers based on simulcast layer matching
        tracing::debug!("Forwarding RTP packet: SSRC={}, PT={} to {} subscribers",
                       rtp_packet.header.ssrc, rtp_packet.header.payload_type, self.subscribers.len());

        // Find which simulcast layer this RTP packet belongs to
        let packet_layer = self.find_simulcast_layer_by_ssrc(rtp_packet.header.ssrc);

        for subscriber_entry in self.subscribers.iter() {
            let subscriber = subscriber_entry.value();
            let subscriber_quality = subscriber.get_preferred_quality();

            // Check if this RTP packet matches the subscriber's preferred quality
            let should_forward = if let Some(layer) = &packet_layer {
                // Forward if the packet's layer matches subscriber's preference or is the best available
                layer.quality == subscriber_quality ||
                self.get_best_layer_for_subscriber(subscriber_quality)
                    .map(|best| best.quality == layer.quality)
                    .unwrap_or(false)
            } else {
                // If we can't identify the layer, forward to all (fallback behavior)
                true
            };

            if should_forward {
                // Send RTP packet to subscriber
                if let Err(e) = subscriber.receive_rtp_packet(&rtp_packet).await {
                    tracing::error!("Failed to forward RTP to subscriber {}: {}",
                                  subscriber.participant_id, e);
                }
            }
        }
    }

    /// Find simulcast layer by SSRC
    fn find_simulcast_layer_by_ssrc(&self, ssrc: u32) -> Option<SimulcastLayer> {
        for layer_entry in self.simulcast_layers.iter() {
            let layer = layer_entry.value();
            if layer.ssrc == ssrc {
                return Some(layer.clone());
            }
        }
        None
    }

    /// Handle RTCP feedback for quality adaptation
    async fn handle_rtcp_feedback(&self, rtcp: &str0m::rtcp::Rtcp) {
        // TODO: Parse Transport-CC feedback and update network conditions
        // This is where we would extract bandwidth estimation, RTT, packet loss, etc.
        // from RTCP feedback reports and update the quality manager

        // For now, create dummy network conditions for demonstration
        let conditions = NetworkConditions {
            available_bandwidth: 1_500_000, // 1.5 Mbps
            rtt_ms: 75.0,
            packet_loss: 0.02, // 2%
            jitter_ms: 15.0,
            last_update: std::time::Instant::now(),
        };

        self.update_network_conditions(conditions);
    }
}
