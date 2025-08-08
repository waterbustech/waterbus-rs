use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use dashmap::DashMap;
use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;
use str0m::{Rtc, Event, Input, Output, IceConnectionState, media::{Direction, MediaKind}};

use crate::{
    models::{
        quality::TrackQuality,
        params::{IceCandidateCallback, RenegotiationCallback, RtcManagerConfigs},
    },
    errors::RtcError,
};

use super::track::Track;

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
    pub ice_candidate_callback: Option<IceCandidateCallback>,
    pub renegotiation_callback: Option<RenegotiationCallback>,
}

impl Subscriber {
    pub async fn new(
        participant_id: String,
        target_id: String,
        ice_candidate_callback: IceCandidateCallback,
        renegotiation_callback: RenegotiationCallback,
        configs: RtcManagerConfigs,
    ) -> Result<Arc<Self>, RtcError> {
        // Create str0m RTC instance
        let mut rtc = Rtc::builder().build();

        // Add local candidate for the shared UDP socket
        // TODO: This should be provided by the UDP socket manager
        let host_addr = if configs.public_ip.is_empty() {
            "127.0.0.1".to_string()
        } else {
            configs.public_ip.clone()
        };

        // For now, use a placeholder port - this will be replaced by the actual UDP socket manager
        let addr = format!("{}:0", host_addr).parse()
            .map_err(|_| RtcError::InvalidIceCandidate)?;
        let candidate = str0m::Candidate::host(addr, str0m::net::Protocol::Udp)
            .map_err(|_| RtcError::InvalidIceCandidate)?;
        rtc.add_local_candidate(candidate);

        let subscriber = Arc::new(Self {
            participant_id,
            target_id,
            rtc: Arc::new(RwLock::new(rtc)),
            cancel_token: CancellationToken::new(),
            preferred_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            network_stats: Arc::new(RwLock::new(NetworkStats::default())),
            tracks: Arc::new(DashMap::new()),
            client_requested_quality: Arc::new(RwLock::new(None)),
            ice_candidate_callback: Some(ice_candidate_callback),
            renegotiation_callback: Some(renegotiation_callback),
        });

        // Start the RTC event loop
        let subscriber_clone = Arc::clone(&subscriber);
        tokio::spawn(async move {
            subscriber_clone.run_rtc_loop().await;
        });

        Ok(subscriber)
    }

    pub fn handle_offer(&self, offer_sdp: String) -> Result<String, RtcError> {
        let mut rtc = self.rtc.write();

        // Parse and set remote offer
        let offer: str0m::change::SdpOffer =
            str0m::change::SdpOffer::from_sdp_string(&offer_sdp).map_err(|_| RtcError::FailedToSetSdp)?;

        let answer = rtc.sdp_api().accept_offer(offer)
            .map_err(|e| RtcError::Str0mError(e))?;

        // Convert answer to SDP string
        let answer_sdp = answer.to_sdp_string();

        Ok(answer_sdp)
    }

    pub fn create_offer(&self) -> Result<String, RtcError> {
        let mut rtc = self.rtc.write();

        // Add media tracks for receiving
        let mut changes = rtc.sdp_api();

        // Add video track for receiving
        let _video_mid = changes.add_media(
            MediaKind::Video,
            Direction::RecvOnly,
            None,
            None,
            None,
        );

        // Add audio track for receiving
        let _audio_mid = changes.add_media(
            MediaKind::Audio,
            Direction::RecvOnly,
            None,
            None,
            None,
        );

        let (offer, _pending) = changes.apply()
            .ok_or(RtcError::FailedToCreateOffer)?;

        // Convert offer to SDP string
        let offer_sdp = offer.to_sdp_string();

        Ok(offer_sdp)
    }

    pub fn set_preferred_quality(&self, quality: TrackQuality) {
        self.preferred_quality.store(quality.as_u8(), Ordering::Relaxed);
        *self.client_requested_quality.write() = Some(quality);
        
        // TODO: Request quality change from publisher
        tracing::debug!("Subscriber {} requested quality: {:?}", self.participant_id, quality);
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
        self.tracks.iter().map(|entry| Arc::clone(entry.value())).collect()
    }

    pub fn close(&self) {
        self.cancel_token.cancel();
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
                tracing::info!("Subscriber {} connected to {}", self.participant_id, self.target_id);
            }
            Event::IceConnectionStateChange(state) => {
                tracing::debug!("ICE connection state changed: {:?}", state);
                if matches!(state, IceConnectionState::Disconnected) {
                    tracing::info!("Subscriber {} disconnected from {}", self.participant_id, self.target_id);
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

    pub async fn receive_rtp_packet(&self, rtp_packet: &str0m::rtp::RtpPacket) -> Result<(), RtcError> {
        // Process incoming RTP packet from publisher
        tracing::debug!("Subscriber {} received RTP packet: SSRC={}, PT={}",
                       self.participant_id, rtp_packet.header.ssrc, rtp_packet.header.payload_type);

        // TODO: Forward the RTP packet through the subscriber's RTC instance
        // This would involve creating the appropriate input for str0m

        Ok(())
    }
}
