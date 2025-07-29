use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant},
    collections::HashMap,
    net::SocketAddr,
};

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use tracing::{warn, info, debug};
use str0m::{
    Rtc, RtcConfig, Candidate, Input, Output, Event, IceConnectionState,
    media::{Direction, MediaKind, Mid, KeyframeRequest, KeyframeRequestKind, MediaData},
    channel::{ChannelId, ChannelData},
    change::{SdpOffer, SdpAnswer},
    net::{Receive, Protocol},
};
use serde_json;

use crate::{
    entities::{media::Media, publisher::Publisher, subscriber::Subscriber},
    errors::WebRTCError,
    models::{
        connection_type::ConnectionType,
        params::{
            AddTrackResponse, IceCandidate, JoinRoomParams, JoinRoomResponse,
            SubscribeHlsLiveStreamParams, SubscribeHlsLiveStreamResponse, SubscribeParams,
            SubscribeResponse, TrackMutexWrapper, WebRTCManagerConfigs,
        },
        streaming_protocol::StreamingProtocol,
    },
};

#[derive(Clone)]
pub struct Room {
    publishers: Arc<DashMap<String, Arc<Publisher>>>,
    subscribers: Arc<DashMap<String, Arc<Subscriber>>>,
    configs: WebRTCManagerConfigs,
    // str0m-specific fields
    rtc_instances: Arc<DashMap<String, Arc<Mutex<Rtc>>>>,
    media_tracks: Arc<DashMap<String, Arc<RwLock<Media>>>>,
}

impl Room {
    pub fn new(configs: WebRTCManagerConfigs) -> Self {
        Self {
            publishers: Arc::new(DashMap::new()),
            subscribers: Arc::new(DashMap::new()),
            rtc_instances: Arc::new(DashMap::new()),
            media_tracks: Arc::new(DashMap::new()),
            configs,
        }
    }

    pub async fn join_room(
        &mut self,
        params: JoinRoomParams,
        room_id: &str,
    ) -> Result<Option<JoinRoomResponse>, WebRTCError> {
        let participant_id = params.participant_id.clone();

        // Create str0m RTC instance
        let mut rtc = Rtc::builder().build();

        // Add local candidate based on configuration
        let local_addr: SocketAddr = format!("{}:{}", 
            if self.configs.public_ip.is_empty() { "0.0.0.0" } else { &self.configs.public_ip },
            self.configs.port_min
        ).parse().map_err(|_| WebRTCError::FailedToCreatePeer)?;
        
        let candidate = Candidate::host(local_addr, "udp")
            .map_err(|_| WebRTCError::FailedToCreatePeer)?;
        rtc.add_local_candidate(candidate);

        // Parse the incoming SDP offer
        let offer: SdpOffer = serde_json::from_str(&params.sdp)
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        // Accept the offer and create answer
        let answer = rtc.accept_offer(offer)
            .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

        // Create media entity
        let media = Media::new(
            participant_id.clone(),
            params.is_video_enabled,
            params.is_audio_enabled,
            params.is_e2ee_enabled,
            params.streaming_protocol,
        );

        // Create publisher
        let publisher = Publisher::new(
            Arc::new(RwLock::new(media.clone())),
            Arc::new(Mutex::new(rtc)),
            params.connection_type.clone(),
        ).await;

        // Store instances
        self._add_publisher(&participant_id, &publisher);
        self.media_tracks.insert(participant_id.clone(), Arc::new(RwLock::new(media)));

        // Handle the joined callback
        let is_migrate = params.connection_type == ConnectionType::P2P;
        tokio::spawn(async move {
            (params.callback)(is_migrate).await;
        });

        let answer_sdp = serde_json::to_string(&answer)
            .map_err(|_| WebRTCError::FailedToGetSdp)?;

        Ok(Some(JoinRoomResponse {
            sdp: answer_sdp,
            is_recording: false,
        }))
    }

    pub async fn subscribe(
        &mut self,
        params: SubscribeParams,
    ) -> Result<SubscribeResponse, WebRTCError> {
        let participant_id = params.participant_id.clone();
        let target_id = params.target_id.clone();

        // Get target's media
        let media_arc = self._get_media(&target_id)?;

        // Create str0m RTC instance for subscriber
        let mut rtc = Rtc::builder().build();

        // Add local candidate
        let local_addr: SocketAddr = format!("{}:{}", 
            if self.configs.public_ip.is_empty() { "0.0.0.0" } else { &self.configs.public_ip },
            self.configs.port_min
        ).parse().map_err(|_| WebRTCError::FailedToCreatePeer)?;
        
        let candidate = Candidate::host(local_addr, "udp")
            .map_err(|_| WebRTCError::FailedToCreatePeer)?;
        rtc.add_local_candidate(candidate);

        // Create change set for subscriber
        let mut change = rtc.create_change_set();
        
        // Add video and audio media based on target's media state
        let (has_video, has_audio) = {
            let media = media_arc.read();
            let state = media.state.read();
            (state.video_enabled, state.audio_enabled)
        };

        if has_video {
            change.add_media(MediaKind::Video, Direction::RecvOnly, None, None);
        }
        if has_audio {
            change.add_media(MediaKind::Audio, Direction::RecvOnly, None, None);
        }

        // Apply changes to get offer
        let (offer, pending) = change.apply()
            .ok_or(WebRTCError::FailedToCreateOffer)?;

        // Create subscriber
        let subscriber = Subscriber::new(
            Arc::new(Mutex::new(rtc)),
            participant_id.clone(),
            target_id.clone(),
            params.on_negotiation_needed,
            params.on_candidate,
        ).await;

        self._add_subscriber(&format!("{}_{}", participant_id, target_id), &subscriber);

        // Extract response from media
        let mut response = self._extract_subscribe_response(&media_arc).await;
        response.offer = serde_json::to_string(&offer)
            .map_err(|_| WebRTCError::FailedToGetSdp)?;

        Ok(response)
    }

    pub fn subscribe_hls_live_stream(
        &self,
        params: SubscribeHlsLiveStreamParams,
    ) -> Result<SubscribeHlsLiveStreamResponse, WebRTCError> {
        // For now, return empty HLS URLs since we're focusing on WebRTC functionality
        Ok(SubscribeHlsLiveStreamResponse {
            hls_urls: vec![],
        })
    }

    pub fn set_subscriber_remote_sdp(
        &self,
        target_id: &str,
        participant_id: &str,
        sdp: &str,
    ) -> Result<(), WebRTCError> {
        let subscriber_key = format!("{}_{}", participant_id, target_id);
        let subscriber = self._get_subscriber(&subscriber_key)?;
        
        // Parse SDP answer and apply it
        let answer: SdpAnswer = serde_json::from_str(sdp)
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        subscriber.set_remote_answer(answer)
            .map_err(|_| WebRTCError::FailedToSetSdp)
    }

    pub async fn handle_publisher_renegotiation(
        &self,
        participant_id: &str,
        sdp: &str,
    ) -> Result<String, WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        
        // Parse the offer
        let offer: SdpOffer = serde_json::from_str(sdp)
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        // Handle renegotiation
        let answer = publisher.handle_renegotiation(offer).await
            .map_err(|_| WebRTCError::FailedToRenegotiate)?;

        serde_json::to_string(&answer)
            .map_err(|_| WebRTCError::FailedToGetSdp)
    }

    pub async fn handle_migrate_connection(
        &self,
        participant_id: &str,
        sdp: &str,
        connection_type: ConnectionType,
    ) -> Result<Option<String>, WebRTCError> {
        // For migration, we need to update the connection type and potentially renegotiate
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_connection_type(connection_type);

        // Parse and handle the SDP
        let offer: SdpOffer = serde_json::from_str(sdp)
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        let answer = publisher.handle_migration(offer).await
            .map_err(|_| WebRTCError::FailedToMigrateConnection)?;

        Ok(Some(serde_json::to_string(&answer)
            .map_err(|_| WebRTCError::FailedToGetSdp)?))
    }

    pub fn add_publisher_candidate(
        &self,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        
        // Convert IceCandidate to str0m Candidate
        let str0m_candidate = self._ice_candidate_to_str0m(candidate)?;
        
        publisher.add_remote_candidate(str0m_candidate)
            .map_err(|_| WebRTCError::FailedToAddCandidate)
    }

    pub fn add_subscriber_candidate(
        &self,
        target_id: &str,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let subscriber_key = format!("{}_{}", participant_id, target_id);
        let subscriber = self._get_subscriber(&subscriber_key)?;
        
        // Convert IceCandidate to str0m Candidate
        let str0m_candidate = self._ice_candidate_to_str0m(candidate)?;
        
        subscriber.add_remote_candidate(str0m_candidate)
            .map_err(|_| WebRTCError::FailedToAddCandidate)
    }

    pub fn leave_room(&mut self, participant_id: &str) {
        // Clean up publisher
        if let Some(publisher) = self.publishers.remove(participant_id) {
            publisher.1.close();
        }

        // Clean up related subscribers
        let subscriber_keys: Vec<String> = self.subscribers
            .iter()
            .filter(|entry| entry.key().contains(participant_id))
            .map(|entry| entry.key().clone())
            .collect();

        for key in subscriber_keys {
            if let Some(subscriber) = self.subscribers.remove(&key) {
                subscriber.1.close();
            }
        }

        // Clean up media tracks
        self.media_tracks.remove(participant_id);
        
        // Clean up RTC instances
        self.rtc_instances.remove(participant_id);
    }

    pub fn set_e2ee_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media_arc = self._get_media(participant_id)?;
        let media = media_arc.write();
        let mut state = media.state.write();
        state.is_e2ee_enabled = is_enabled;
        Ok(())
    }

    pub fn set_camera_type(&self, participant_id: &str, camera_type: u8) -> Result<(), WebRTCError> {
        let media_arc = self._get_media(participant_id)?;
        let media = media_arc.write();
        let mut state = media.state.write();
        state.camera_type = camera_type;
        Ok(())
    }

    pub fn set_video_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media_arc = self._get_media(participant_id)?;
        let media = media_arc.write();
        let mut state = media.state.write();
        state.video_enabled = is_enabled;
        Ok(())
    }

    pub fn set_audio_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media_arc = self._get_media(participant_id)?;
        let media = media_arc.write();
        let mut state = media.state.write();
        state.audio_enabled = is_enabled;
        Ok(())
    }

    pub fn set_screen_sharing(
        &self,
        participant_id: &str,
        is_enabled: bool,
        screen_track_id: Option<String>,
    ) -> Result<(), WebRTCError> {
        let media_arc = self._get_media(participant_id)?;
        let media = media_arc.write();
        let mut state = media.state.write();
        state.is_screen_sharing = is_enabled;
        state.screen_track_id = screen_track_id;
        Ok(())
    }

    pub fn set_hand_raising(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media_arc = self._get_media(participant_id)?;
        let media = media_arc.write();
        let mut state = media.state.write();
        state.is_hand_raising = is_enabled;
        Ok(())
    }

    // Private helper methods
    #[inline]
    fn _add_publisher(&self, participant_id: &str, publisher: &Arc<Publisher>) {
        self.publishers.insert(participant_id.to_string(), Arc::clone(publisher));
    }

    #[inline]
    fn _add_subscriber(&self, key: &str, subscriber: &Arc<Subscriber>) {
        self.subscribers.insert(key.to_string(), Arc::clone(subscriber));
    }

    #[inline]
    fn _get_publisher(&self, participant_id: &str) -> Result<Arc<Publisher>, WebRTCError> {
        self.publishers
            .get(participant_id)
            .map(|p| p.clone())
            .ok_or(WebRTCError::ParticipantNotFound)
    }

    #[inline]
    fn _get_subscriber(&self, key: &str) -> Result<Arc<Subscriber>, WebRTCError> {
        self.subscribers
            .get(key)
            .map(|s| s.clone())
            .ok_or(WebRTCError::ParticipantNotFound)
    }

    #[inline]
    fn _get_media(&self, participant_id: &str) -> Result<Arc<RwLock<Media>>, WebRTCError> {
        self.media_tracks
            .get(participant_id)
            .map(|m| m.clone())
            .ok_or(WebRTCError::ParticipantNotFound)
    }

    #[inline]
    fn _ice_candidate_to_str0m(&self, candidate: IceCandidate) -> Result<Candidate, WebRTCError> {
        // Parse the candidate string to extract the address
        // This is a simplified conversion - a full implementation would need proper parsing
        let parts: Vec<&str> = candidate.candidate.split_whitespace().collect();
        if parts.len() >= 5 {
            let ip = parts[4];
            let port = parts[5].parse::<u16>().map_err(|_| WebRTCError::FailedToAddCandidate)?;
            let addr: SocketAddr = format!("{}:{}", ip, port).parse()
                .map_err(|_| WebRTCError::FailedToAddCandidate)?;
            
            Candidate::host(addr, "udp").map_err(|_| WebRTCError::FailedToAddCandidate)
        } else {
            Err(WebRTCError::FailedToAddCandidate)
        }
    }

    #[inline]
    async fn _extract_subscribe_response(
        &self,
        media_arc: &Arc<RwLock<Media>>,
    ) -> SubscribeResponse {
        let media = media_arc.read();
        let media_state = media.state.read();

        SubscribeResponse {
            camera_type: media_state.camera_type,
            video_enabled: media_state.video_enabled,
            audio_enabled: media_state.audio_enabled,
            is_hand_raising: media_state.is_hand_raising,
            is_e2ee_enabled: media_state.is_e2ee_enabled,
            is_screen_sharing: media_state.is_screen_sharing,
            screen_track_id: media_state.screen_track_id.clone(),
            video_codec: media_state.codec.clone(),
            offer: String::new(), // Will be filled by caller
        }
    }
}
