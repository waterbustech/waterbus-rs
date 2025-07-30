use std::{net::UdpSocket, sync::Arc};

use dashmap::DashMap;
use parking_lot::RwLock;
use str0m::{Candidate, Rtc};
use tracing::warn;

use crate::{
    entities::{media::Media, publisher::Publisher},
    errors::WebRTCError,
    models::{
        connection_type::ConnectionType,
        input_params::{
            IceCandidate, JoinRoomParams, JoinRoomResponse, RtcManagerConfig,
            SubscribeHlsLiveStreamParams, SubscribeHlsLiveStreamResponse, SubscribeParams,
            SubscribeResponse,
        },
        streaming_protocol::StreamingProtocol,
    },
    utils::select_host_address::select_host_address,
};

#[derive(Clone)]
pub struct Room {
    publishers: Arc<DashMap<String, Arc<Publisher>>>,
    udp_socket: UdpSocket,
    config: RtcManagerConfig,
}

impl Room {
    pub fn new(config: RtcManagerConfig) -> Self {
        let host_addr = select_host_address();
        let udp_socket = UdpSocket::bind(host_addr).unwrap();

        Self {
            publishers: Arc::new(DashMap::new()),
            udp_socket,
            config,
        }
    }

    pub async fn join_room(
        &mut self,
        params: JoinRoomParams,
        room_id: &str,
    ) -> Result<Option<JoinRoomResponse>, WebRTCError> {
        let participant_id = params.participant_id.clone();

        let rtc = self._create_rtc(params.is_ipv6_supported).await?;

        // Create publisher with the new str0m-based architecture
        let publisher = Publisher::new(rtc, params).await;

        self._add_publisher(&participant_id, &publisher);

        // Handle P2P vs SFU logic
        if params.connection_type == ConnectionType::P2P {
            // For P2P, we cache the SDP and don't return an answer immediately
            let mut media = publisher.media.write();
            media.cache_sdp(params.sdp.clone());

            // Execute callback immediately for P2P
            tokio::spawn(async move {
                (params.callback)(false).await;
            });

            Ok(None)
        } else {
            // For SFU, we need to handle SDP negotiation
            let sdp_offer = serde_json::from_str::<str0m::change::SdpOffer>(&params.sdp)
                .map_err(|_| WebRTCError::FailedToCreateOffer)?;

            let answer = rtc
                .sdp_api()
                .accept_offer(sdp_offer)
                .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

            let answer_json =
                serde_json::to_string(&answer).map_err(|_| WebRTCError::FailedToCreateAnswer)?;

            Ok(Some(JoinRoomResponse {
                sdp: answer_json,
                is_recording: false,
            }))
        }
    }

    pub async fn subscribe(
        &mut self,
        params: SubscribeParams,
    ) -> Result<SubscribeResponse, WebRTCError> {
        let target_id = &params.target_id;
        let participant_id = &params.participant_id;

        // Get the target publisher
        let publisher = self._get_publisher(target_id)?;

        // Check if we have cached SDP for P2P
        let cached_sdp = {
            let media = publisher.media.read();
            media.get_sdp()
        };

        if let Some(sdp) = cached_sdp {
            // Return cached SDP for P2P connections
            let media_state = publisher.get_media_state();

            Ok(SubscribeResponse {
                offer: sdp,
                camera_type: media_state.camera_type,
                video_enabled: media_state.video_enabled,
                audio_enabled: media_state.audio_enabled,
                is_screen_sharing: media_state.is_screen_sharing,
                is_hand_raising: media_state.is_hand_raising,
                is_e2ee_enabled: media_state.is_e2ee_enabled,
                video_codec: media_state.codec,
                screen_track_id: media_state.screen_track_id,
            })
        } else {
            // For SFU, create a new subscriber
            let connection_type = publisher.get_connection_type();

            if connection_type == ConnectionType::P2P {
                return Err(WebRTCError::PeerNotFound);
            }

            // Create subscriber through the publisher
            let subscriber = publisher.subscribe_to_publisher(params).await?;

            // Get media state for response
            let media_state = publisher.get_media_state();

            // Create SDP offer for the subscriber
            let mut change = subscriber.rtc.sdp_api();

            // Add media for each track in the publisher
            let media = publisher.media.read();
            for track_entry in media.tracks.iter() {
                let (mid, track_info) = track_entry.pair();
                let stream_id = track_info.origin.clone();
                let new_mid = change.add_media(
                    track_info.kind,
                    str0m::media::Direction::SendOnly,
                    Some(stream_id),
                    None,
                    None,
                );

                // Update subscriber's track state
                subscriber.tracks_out.insert(
                    *mid,
                    crate::entities::media::TrackOutState::Negotiating(new_mid),
                );
            }

            let (offer, _pending) = change.apply().ok_or(WebRTCError::FailedToCreateOffer)?;

            let offer_json =
                serde_json::to_string(&offer).map_err(|_| WebRTCError::FailedToCreateOffer)?;

            Ok(SubscribeResponse {
                offer: offer_json,
                camera_type: media_state.camera_type,
                video_enabled: media_state.video_enabled,
                audio_enabled: media_state.audio_enabled,
                is_screen_sharing: media_state.is_screen_sharing,
                is_hand_raising: media_state.is_hand_raising,
                is_e2ee_enabled: media_state.is_e2ee_enabled,
                video_codec: media_state.codec,
                screen_track_id: media_state.screen_track_id,
            })
        }
    }

    pub fn subscribe_hls_live_stream(
        &self,
        params: SubscribeHlsLiveStreamParams,
    ) -> Result<SubscribeHlsLiveStreamResponse, WebRTCError> {
        let target_id = &params.target_id;

        let publisher = self._get_publisher(target_id)?;
        let media = publisher.media.read();

        if media.streaming_protocol != StreamingProtocol::HLS {
            return Err(WebRTCError::InvalidStreamingProtocol);
        }

        let hls_urls = media.get_hls_urls();

        Ok(SubscribeHlsLiveStreamResponse { hls_urls })
    }

    pub fn set_subscriber_remote_sdp(
        &self,
        target_id: &str,
        participant_id: &str,
        sdp: &str,
    ) -> Result<(), WebRTCError> {
        // In str0m, we handle SDP negotiation differently
        // This would be implemented based on your specific needs
        Ok(())
    }

    pub async fn handle_publisher_renegotiation(
        &self,
        participant_id: &str,
        sdp: &str,
    ) -> Result<String, WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        let rtc = &publisher.rtc;

        let offer = serde_json::from_str::<str0m::change::SdpOffer>(sdp)
            .map_err(|_| WebRTCError::FailedToCreateOffer)?;

        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

        let answer_json =
            serde_json::to_string(&answer).map_err(|_| WebRTCError::FailedToCreateAnswer)?;

        Ok(answer_json)
    }

    pub async fn handle_migrate_connection(
        &self,
        participant_id: &str,
        sdp: &str,
        connection_type: ConnectionType,
    ) -> Result<Option<String>, WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;

        publisher.set_connection_type(connection_type.clone());

        if connection_type == ConnectionType::SFU {
            let rtc = &publisher.rtc;

            let offer = serde_json::from_str::<str0m::change::SdpOffer>(sdp)
                .map_err(|_| WebRTCError::FailedToCreateOffer)?;

            let answer = rtc
                .sdp_api()
                .accept_offer(offer)
                .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

            let answer_json =
                serde_json::to_string(&answer).map_err(|_| WebRTCError::FailedToCreateAnswer)?;

            Ok(Some(answer_json))
        } else {
            // For P2P, cache the SDP
            let mut media = publisher.media.write();
            media.cache_sdp(sdp.to_owned());
            Ok(None)
        }
    }

    pub fn add_publisher_candidate(
        &self,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        let rtc = &publisher.rtc;

        // In str0m, ICE candidates are handled automatically
        // This method might not be needed or would be implemented differently
        Ok(())
    }

    pub fn add_subscriber_candidate(
        &self,
        target_id: &str,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        // In str0m, ICE candidates are handled automatically
        // This method might not be needed or would be implemented differently
        Ok(())
    }

    #[inline]
    pub fn leave_room(&mut self, participant_id: &str) {
        if let Some((_id, publisher)) = self.publishers.remove(participant_id) {
            publisher.close();
        }
    }

    #[inline]
    pub fn set_e2ee_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_e2ee_enabled(is_enabled);
        Ok(())
    }

    #[inline]
    pub fn set_camera_type(
        &self,
        participant_id: &str,
        camera_type: u8,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_camera_type(camera_type);
        Ok(())
    }

    #[inline]
    pub fn set_video_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_video_enabled(is_enabled);
        Ok(())
    }

    #[inline]
    pub fn set_audio_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_audio_enabled(is_enabled);
        Ok(())
    }

    #[inline]
    pub fn set_screen_sharing(
        &self,
        participant_id: &str,
        is_enabled: bool,
        screen_track_id: Option<String>,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_screen_sharing(is_enabled, screen_track_id);
        Ok(())
    }

    #[inline]
    pub fn set_hand_raising(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_hand_raising(is_enabled);
        Ok(())
    }

    #[inline]
    fn _get_publisher(&self, participant_id: &str) -> Result<Arc<Publisher>, WebRTCError> {
        let result = self
            .publishers
            .get(participant_id)
            .map(|r| r.clone())
            .ok_or(WebRTCError::ParticipantNotFound)?;

        Ok(result)
    }

    #[inline]
    fn _add_publisher(&self, participant_id: &str, publisher: &Arc<Publisher>) {
        self.publishers
            .insert(participant_id.to_owned(), publisher.clone());
    }

    pub async fn _create_rtc(&self, is_ipv6_supported: bool) -> Result<Arc<Rtc>, WebRTCError> {
        let mut rtc = Rtc::builder().set_ice_lite(true).build();

        let addr = self.udp_socket.local_addr().unwrap();

        let candidate = Candidate::host(addr, "udp").expect("a host candidate");
        rtc.add_local_candidate(candidate).unwrap();

        Ok(Arc::new(rtc))
    }
}
