use std::sync::Arc;
use parking_lot::RwLock;

use dashmap::DashMap;
use str0m::{Candidate, net::Protocol};
use std::net::SocketAddr;

use crate::{
    entities::{publisher::Publisher, subscriber::Subscriber},
    errors::RtcError,
    models::{
        connection_type::ConnectionType,
        params::{
            IceCandidate, JoinRoomParams, JoinRoomResponse,
            SubscribeHlsLiveStreamParams, SubscribeHlsLiveStreamResponse, SubscribeParams,
            SubscribeResponse, RtcManagerConfigs,
        },

    },
    services::udp_socket_manager::UdpSocketManager,
};

#[derive(Clone)]
pub struct Room {
    pub room_id: String,
    publishers: Arc<DashMap<String, Arc<Publisher>>>,
    subscribers: Arc<DashMap<String, Arc<Subscriber>>>,
    configs: RtcManagerConfigs,
    udp: Arc<RwLock<UdpSocketManager>>, // access to shared UDP
}

impl Room {
    pub fn new(room_id: String, configs: RtcManagerConfigs, udp: Arc<RwLock<UdpSocketManager>>) -> Self {
        Self {
            room_id,
            publishers: Arc::new(DashMap::new()),
            subscribers: Arc::new(DashMap::new()),
            configs,
            udp,
        }
    }

    pub async fn join_room(
        &mut self,
        params: JoinRoomParams,
        _room_id: &str,
    ) -> Result<Option<JoinRoomResponse>, RtcError> {
        let participant_id = params.participant_id.clone();

        // Create publisher
        let publisher = Publisher::new(
            participant_id.clone(),
            self.room_id.clone(),
            params.connection_type,
            params.is_video_enabled,
            params.is_audio_enabled,
            params.is_e2ee_enabled,
            params.streaming_protocol,
            params.on_candidate,
            params.callback,
            self.configs.clone(),
            self.udp.clone(),
        ).await?;

        // Add publisher to room
        self.publishers.insert(participant_id.clone(), publisher.clone());

        // Handle SDP based on connection type
        match params.connection_type {
            ConnectionType::SFU => {
                // For SFU mode, handle the offer and create an answer
                let answer_sdp = publisher.handle_offer(params.sdp)?;

                Ok(Some(JoinRoomResponse {
                    sdp: answer_sdp,
                    is_recording: false,
                }))
            }
            ConnectionType::P2P => {
                // For P2P mode, cache the SDP and return None
                // The callback will be called when the connection is established
                Ok(None)
            }
        }
    }

    pub async fn subscribe(
        &mut self,
        params: SubscribeParams,
    ) -> Result<SubscribeResponse, RtcError> {
        let target_id = params.target_id.clone();
        let participant_id = params.participant_id.clone();

        // Get the target publisher
        let publisher = self.publishers
            .get(&target_id)
            .ok_or(RtcError::PublisherNotFound)?
            .clone();

        // Create subscriber
        let subscriber = Subscriber::new(
            participant_id.clone(),
            target_id.clone(),
            params.on_candidate,
            params.on_negotiation_needed,
            self.configs.clone(),
            self.udp.clone(),
        ).await?;

        // Add subscriber to publisher's subscriber list
        publisher.add_subscriber(participant_id.clone(), subscriber.clone());

        // Create subscriber key for room-level tracking
        let subscriber_key = format!("{}_{}", target_id, participant_id);
        self.subscribers.insert(subscriber_key, subscriber.clone());

        // Create offer for the subscriber
        let offer_sdp = subscriber.create_offer()?;

        Ok(SubscribeResponse {
            sdp: offer_sdp.clone(),
            offer: offer_sdp,
            camera_type: 0, // Default camera type
            video_enabled: true, // TODO: Get from publisher state
            audio_enabled: true, // TODO: Get from publisher state
            is_screen_sharing: false, // TODO: Get from publisher state
            is_hand_raising: false, // TODO: Get from publisher state
            is_e2ee_enabled: false, // TODO: Get from publisher state
            video_codec: "VP8".to_string(), // TODO: Get from publisher
            screen_track_id: "".to_string(), // TODO: Get from publisher if screen sharing
        })
    }

    pub fn subscribe_hls_live_stream(
        &self,
        _params: SubscribeHlsLiveStreamParams,
    ) -> Result<SubscribeHlsLiveStreamResponse, RtcError> {
        // TODO: Implement HLS live stream subscription
        Ok(SubscribeHlsLiveStreamResponse {
            playlist_url: "".to_string(),
        })
    }

    pub fn add_publisher_candidate(
        &self,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), RtcError> {
        let publisher = self.publishers
            .get(participant_id)
            .ok_or(RtcError::PublisherNotFound)?;

        // Convert IceCandidate to str0m Candidate
        let str0m_candidate = self.convert_ice_candidate_to_str0m(candidate.clone())?;

        // Add candidate to publisher's RTC instance
        {
            let mut rtc = publisher.rtc.write();
            rtc.add_local_candidate(str0m_candidate);
        }

        // Register RTC with UDP manager based on remote address (from candidate)
        let parts: Vec<&str> = candidate.candidate.split_whitespace().collect();
        if parts.len() >= 6 {
            let remote_str = format!("{}:{}", parts[4], parts[5]);
            if let Ok(remote_addr) = remote_str.parse::<SocketAddr>() {
                self.udp.read().register_rtc(remote_addr, publisher.rtc.clone());
            }
        }

        Ok(())
    }

    pub fn add_subscriber_candidate(
        &self,
        target_id: &str,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), RtcError> {
        let subscriber_key = format!("{}_{}", target_id, participant_id);
        let subscriber = self.subscribers
            .get(&subscriber_key)
            .ok_or(RtcError::SubscriberNotFound)?;

        // Convert IceCandidate to str0m Candidate
        let str0m_candidate = self.convert_ice_candidate_to_str0m(candidate.clone())?;

        // Add candidate to subscriber's RTC instance
        {
            let mut rtc = subscriber.rtc.write();
            rtc.add_local_candidate(str0m_candidate);
        }

        // Register RTC with UDP manager based on remote address (from candidate)
        let parts: Vec<&str> = candidate.candidate.split_whitespace().collect();
        if parts.len() >= 6 {
            let remote_str = format!("{}:{}", parts[4], parts[5]);
            if let Ok(remote_addr) = remote_str.parse::<SocketAddr>() {
                self.udp.read().register_rtc(remote_addr, subscriber.rtc.clone());
            }
        }

        Ok(())
    }

    pub fn set_subscriber_sdp(
        &self,
        target_id: &str,
        participant_id: &str,
        sdp: String,
    ) -> Result<(), RtcError> {
        let subscriber_key = format!("{}_{}", target_id, participant_id);
        let _subscriber = self.subscribers
            .get(&subscriber_key)
            .ok_or(RtcError::SubscriberNotFound)?;

        // TODO: Handle SDP answer from subscriber
        tracing::debug!("Setting SDP for subscriber {}: {}", subscriber_key, sdp);
        
        Ok(())
    }

    pub fn publisher_renegotiation(
        &self,
        participant_id: &str,
        sdp: String,
    ) -> Result<String, RtcError> {
        let _publisher = self.publishers
            .get(participant_id)
            .ok_or(RtcError::PublisherNotFound)?;

        // TODO: Handle publisher renegotiation
        tracing::debug!("Publisher {} renegotiation: {}", participant_id, sdp);
        
        // For now, return the same SDP
        Ok(sdp)
    }

    pub fn migrate_connection(
        &self,
        participant_id: &str,
        sdp: String,
        _connection_type: ConnectionType,
    ) -> Result<String, RtcError> {
        let _publisher = self.publishers
            .get(participant_id)
            .ok_or(RtcError::PublisherNotFound)?;

        // TODO: Handle connection migration
        tracing::debug!("Migrating connection for participant {}: {}", participant_id, sdp);
        
        // For now, return the same SDP
        Ok(sdp)
    }

    pub fn leave_room(&mut self, participant_id: &str) {
        // Remove all subscribers targeting this participant
        self.remove_all_subscribers_with_target_id(participant_id);

        // Remove and close the publisher
        if let Some((_, publisher)) = self.publishers.remove(participant_id) {
            publisher.close();
        }
    }

    pub fn set_e2ee_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), RtcError> {
        let publisher = self.publishers
            .get(participant_id)
            .ok_or(RtcError::PublisherNotFound)?;

        publisher.set_e2ee_enabled(is_enabled);
        Ok(())
    }

    pub fn set_camera_type(
        &self,
        participant_id: &str,
        _camera_type: u8,
    ) -> Result<(), RtcError> {
        let _publisher = self.publishers
            .get(participant_id)
            .ok_or(RtcError::PublisherNotFound)?;

        // TODO: Implement camera type setting
        tracing::debug!("Setting camera type for participant {}", participant_id);
        Ok(())
    }

    pub fn set_video_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), RtcError> {
        let publisher = self.publishers
            .get(participant_id)
            .ok_or(RtcError::PublisherNotFound)?;

        publisher.set_video_enabled(is_enabled);
        Ok(())
    }

    pub fn set_audio_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), RtcError> {
        let publisher = self.publishers
            .get(participant_id)
            .ok_or(RtcError::PublisherNotFound)?;

        publisher.set_audio_enabled(is_enabled);
        Ok(())
    }

    pub fn set_screen_sharing(
        &self,
        participant_id: &str,
        _is_sharing: bool,
        _screen_track_id: Option<String>,
    ) -> Result<(), RtcError> {
        let _publisher = self.publishers
            .get(participant_id)
            .ok_or(RtcError::PublisherNotFound)?;

        // TODO: Implement screen sharing
        tracing::debug!("Setting screen sharing for participant {}", participant_id);
        Ok(())
    }

    pub fn set_hand_raising(
        &self,
        participant_id: &str,
        _is_raising: bool,
    ) -> Result<(), RtcError> {
        let _publisher = self.publishers
            .get(participant_id)
            .ok_or(RtcError::PublisherNotFound)?;

        // TODO: Implement hand raising
        tracing::debug!("Setting hand raising for participant {}", participant_id);
        Ok(())
    }

    fn remove_all_subscribers_with_target_id(&self, target_id: &str) {
        let keys_to_remove: Vec<String> = self.subscribers
            .iter()
            .filter(|entry| entry.key().starts_with(&format!("{}_", target_id)))
            .map(|entry| entry.key().clone())
            .collect();

        for key in keys_to_remove {
            if let Some((_, subscriber)) = self.subscribers.remove(&key) {
                subscriber.close();
            }
        }

        // Also remove subscribers from the publisher
        if let Some(publisher) = self.publishers.get(target_id) {
            let subscriber_ids: Vec<String> = publisher.subscribers
                .iter()
                .map(|entry| entry.key().clone())
                .collect();
            
            for subscriber_id in subscriber_ids {
                publisher.remove_subscriber(&subscriber_id);
            }
        }
    }

    fn convert_ice_candidate_to_str0m(&self, candidate: IceCandidate) -> Result<Candidate, RtcError> {
        // Parse the candidate string to extract address and protocol
        // This is a simplified implementation - in practice, you'd need more robust parsing
        let parts: Vec<&str> = candidate.candidate.split_whitespace().collect();
        if parts.len() < 6 {
            return Err(RtcError::InvalidIceCandidate);
        }

        let address = format!("{}:{}", parts[4], parts[5]);
        let addr = address.parse()
            .map_err(|_| RtcError::InvalidIceCandidate)?;

        let protocol = match parts[2].to_lowercase().as_str() {
            "udp" => Protocol::Udp,
            "tcp" => Protocol::Tcp,
            _ => Protocol::Udp, // Default to UDP
        };

        Candidate::host(addr, protocol)
            .map_err(|_| RtcError::InvalidIceCandidate)
    }

    pub fn get_publisher_count(&self) -> usize {
        self.publishers.len()
    }

    pub fn get_subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    pub fn get_publishers(&self) -> Vec<String> {
        self.publishers.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Set quality preference for a subscriber
    pub fn set_subscriber_quality(
        &self,
        target_id: &str,
        participant_id: &str,
        quality: crate::models::quality::TrackQuality,
    ) -> Result<(), RtcError> {
        let subscriber_key = format!("{}_{}", target_id, participant_id);
        let subscriber = self.subscribers
            .get(&subscriber_key)
            .ok_or(RtcError::SubscriberNotFound)?;

        subscriber.set_preferred_quality(quality);

        tracing::info!("Set quality preference for subscriber {} -> {}: {:?}",
                     participant_id, target_id, quality);

        Ok(())
    }

    /// Get simulcast layers for a publisher
    pub fn get_publisher_simulcast_layers(&self, publisher_id: &str) -> Result<Vec<crate::entities::publisher::SimulcastLayer>, RtcError> {
        let publisher = self.publishers
            .get(publisher_id)
            .ok_or(RtcError::PublisherNotFound)?;

        Ok(publisher.get_simulcast_layers())
    }
}
