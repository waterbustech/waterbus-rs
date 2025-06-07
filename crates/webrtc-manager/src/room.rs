use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use tracing::warn;
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
        setting_engine::SettingEngine,
    },
    ice::{
        network_type::NetworkType,
        udp_network::{EphemeralUDP, UDPNetwork},
    },
    ice_transport::{ice_candidate::RTCIceCandidateInit, ice_candidate_type::RTCIceCandidateType},
    interceptor::registry::Registry,
    peer_connection::{
        RTCPeerConnection,
        configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        policy::{
            bundle_policy::RTCBundlePolicy, ice_transport_policy::RTCIceTransportPolicy,
            rtcp_mux_policy::RTCRtcpMuxPolicy,
        },
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::{
        RTCPFeedback, TYPE_RTCP_FB_GOOG_REMB, TYPE_RTCP_FB_NACK, TYPE_RTCP_FB_TRANSPORT_CC,
        rtp_codec::{RTCRtpHeaderExtensionCapability, RTPCodecType},
    },
};

use crate::{
    entities::{media::Media, publisher::Publisher, subscriber::Subscriber},
    errors::WebRTCError,
    models::{
        connection_type::ConnectionType,
        params::{
            AddTrackResponse, IceCandidate, JoinRoomParams, JoinRoomResponse, SubscribeParams,
            SubscribeResponse, TrackMutexWrapper, WebRTCManagerConfigs,
        },
    },
};

#[derive(Debug, Clone)]
pub struct Room {
    publishers: Arc<DashMap<String, Arc<Publisher>>>,
    subscribers: Arc<DashMap<String, Arc<Subscriber>>>,
    configs: WebRTCManagerConfigs,
}

impl Room {
    pub fn new(configs: WebRTCManagerConfigs) -> Self {
        Self {
            publishers: Arc::new(DashMap::new()),
            subscribers: Arc::new(DashMap::new()),
            configs: configs,
        }
    }

    pub async fn join_room(
        &mut self,
        params: JoinRoomParams,
        room_id: &str,
    ) -> Result<Option<JoinRoomResponse>, WebRTCError> {
        let participant_id = params.participant_id;

        let pc = self._create_pc().await?;

        let mut media = Media::new(
            participant_id.clone(),
            params.is_video_enabled,
            params.is_audio_enabled,
            params.is_e2ee_enabled,
        );

        if params.connection_type == ConnectionType::P2P {
            media.cache_sdp(params.sdp.clone());
        }

        // let _ = media.initialize_hls_writer().await;

        let publisher = Arc::new(Publisher::new(
            Arc::new(RwLock::new(media)),
            pc.clone(),
            params.connection_type.clone(),
        ));
        self._add_publisher(&participant_id, &publisher);

        let is_migrate = params.connection_type == ConnectionType::P2P;

        // === Peer Connection Callbacks ===
        // If total tracks is 0 -> execute joined callback when pc connected
        if params.total_tracks == 0 {
            let has_emitted = Arc::new(Mutex::new(false));
            {
                let peer_clone = pc.clone();
                let callback = params.callback.clone();
                let has_emitted = has_emitted.clone();
                let is_migrate = is_migrate.clone();

                pc.on_peer_connection_state_change(Box::new(move |_| {
                    let peer = peer_clone.clone();
                    let callback = callback.clone();
                    let has_emitted = has_emitted.clone();

                    Box::pin(async move {
                        if peer.connection_state() == RTCPeerConnectionState::Connected {
                            drop(peer);
                            let mut emitted = has_emitted.lock();
                            if !*emitted {
                                *emitted = true;
                                tokio::spawn(async move {
                                    (callback)(is_migrate).await;
                                });
                            }
                        }
                    })
                }));
            }
        }

        // === Media Track ===
        let track_counter = Arc::new(Mutex::new(0u8));
        let callback_called = Arc::new(Mutex::new(false));

        {
            let media = self._get_media(&participant_id)?;
            let room_id = room_id.to_string();
            let participant_id = participant_id.clone();
            let subscribers = Arc::clone(&self.subscribers);
            let publisher = publisher.clone();

            let track_counter = Arc::clone(&track_counter);
            let callback_called = Arc::clone(&callback_called);
            let callback = params.callback.clone();

            let is_migrate = params.connection_type == ConnectionType::P2P;

            pc.on_track(Box::new(move |track, _, _| {
                let media = Arc::clone(&media);
                let subscribers = Arc::clone(&subscribers);
                let room_id = room_id.clone();
                let participant_id = participant_id.clone();
                let track_counter = Arc::clone(&track_counter);
                let callback_called = Arc::clone(&callback_called);
                let callback = callback.clone();

                publisher.send_rtcp_pli(track.ssrc());

                let media = media.write();
                let add_track_response = media.add_track(track, room_id);

                Box::pin(async move {
                    tokio::spawn(async move {
                        let (maybe_track, should_count) = match add_track_response {
                            AddTrackResponse::AddTrackSuccess(track) => (Some(track), true),
                            AddTrackResponse::AddSimulcastTrackSuccess(track) => {
                                (Some(track), false)
                            }
                            AddTrackResponse::FailedToAddTrack => {
                                warn!("Failed to add track");
                                (None, false)
                            }
                        };

                        if let Some(track) = maybe_track {
                            if let Err(e) = Self::_add_track_to_subscribers(
                                Arc::clone(&subscribers),
                                track,
                                &participant_id,
                            )
                            .await
                            {
                                warn!("Failed to add track to subscribers: {:?}", e);
                            }

                            if should_count {
                                let mut count = track_counter.lock();
                                *count += 1;

                                if *count == params.total_tracks {
                                    let mut called = callback_called.lock();
                                    if !*called {
                                        *called = true;

                                        tokio::spawn((callback)(is_migrate));
                                    }
                                }
                            }
                        }
                    });
                })
            }));
        }

        // === ICE Candidate Callback ===
        {
            let on_candidate = params.on_candidate.clone();
            pc.on_ice_candidate(Box::new(move |candidate| {
                let on_candidate = on_candidate.clone();
                Box::pin(async move {
                    if let Some(candidate) = candidate {
                        if let Ok(init) = candidate.to_json() {
                            let ice = IceCandidate {
                                candidate: init.candidate,
                                sdp_mid: init.sdp_mid,
                                sdp_m_line_index: init.sdp_mline_index,
                            };
                            tokio::spawn((on_candidate)(ice));
                        }
                    }
                })
            }));
        }

        // === SDP Exchange ===
        if params.connection_type == ConnectionType::SFU {
            let sdp = RTCSessionDescription::offer(params.sdp.clone())
                .map_err(|_| WebRTCError::FailedToCreateOffer)?;

            pc.set_remote_description(sdp)
                .await
                .map_err(|_| WebRTCError::FailedToSetSdp)?;

            let answer = pc
                .create_answer(None)
                .await
                .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

            pc.set_local_description(answer.clone())
                .await
                .map_err(|_| WebRTCError::FailedToSetSdp)?;

            return Ok(Some(JoinRoomResponse {
                sdp: answer.sdp.clone(),
                is_recording: false,
            }));
        } else {
            let callback = params.callback.clone();
            tokio::spawn(async move {
                (callback)(false).await;
            });
        }

        Ok(None)
    }

    pub async fn subscribe(
        &mut self,
        params: SubscribeParams,
    ) -> Result<SubscribeResponse, WebRTCError> {
        let target_id = &params.target_id;
        let participant_id = &params.participant_id;

        let media_arc = self._get_media(target_id)?;

        let subscribe_response = self._extract_subscribe_response(&media_arc).await;

        let sdp_cached = {
            let mut writer = media_arc.write();
            writer.get_sdp()
        };

        match sdp_cached {
            Some(sdp) => {
                return Ok(SubscribeResponse {
                    offer: sdp,
                    ..subscribe_response
                });
            }
            None => {
                let connection_type = match self._get_publisher(&target_id) {
                    Ok(publisher) => publisher.get_connection_type().clone(),
                    Err(_) => ConnectionType::P2P,
                };

                if connection_type == ConnectionType::P2P {
                    return Err(WebRTCError::PeerNotFound);
                }

                let peer_id = self._get_subscriber_peer_id(target_id, participant_id);

                let pc = self._create_pc().await?;

                self._add_subscriber(&peer_id, &pc, participant_id.clone());

                // Clone for callbacks
                let peer_clone = pc.clone();
                let media_clone = Arc::clone(&media_arc);
                let renegotiation_callback = params.on_negotiation_needed.clone();
                pc.on_negotiation_needed(Box::new(move || {
                    let peer = peer_clone.clone();
                    let media = media_clone.clone();
                    let callback = renegotiation_callback.clone();

                    let need_renegotiate = {
                        let media = media.read();
                        media.tracks.len() > 2
                    };

                    Box::pin(async move {
                        if !need_renegotiate {
                            return;
                        }

                        if let Ok(desc) = peer.create_offer(None).await {
                            let _ = peer.set_local_description(desc.clone()).await;
                            tokio::spawn((callback)(desc.sdp));
                        }
                    })
                }));

                let on_candidate = params.on_candidate.clone();
                pc.on_ice_candidate(Box::new(move |cand| {
                    let callback = on_candidate.clone();
                    Box::pin(async move {
                        if let Some(candidate) = cand {
                            if let Ok(init) = candidate.to_json() {
                                let ice = IceCandidate {
                                    candidate: init.candidate,
                                    sdp_mid: init.sdp_mid,
                                    sdp_m_line_index: init.sdp_mline_index,
                                };
                                tokio::spawn((callback)(ice));
                            } else {
                                warn!("Failed to convert ICE candidate");
                            }
                        }
                    })
                }));

                let subscriber = self._get_subscriber(target_id, participant_id)?;
                let _ = self._forward_all_tracks(subscriber, &media_arc).await;

                // Create and set offer
                let offer_desc = pc
                    .create_offer(None)
                    .await
                    .map_err(|_| WebRTCError::FailedToCreateOffer)?;
                pc.set_local_description(offer_desc.clone())
                    .await
                    .map_err(|_| WebRTCError::FailedToSetSdp)?;

                let local_desc = pc
                    .local_description()
                    .await
                    .ok_or(WebRTCError::FailedToGetSdp)?;

                return Ok(SubscribeResponse {
                    offer: local_desc.sdp.clone(),
                    ..subscribe_response
                });
            }
        }
    }

    pub fn set_subscriber_remote_sdp(
        &self,
        target_id: &str,
        participant_id: &str,
        sdp: &str,
    ) -> Result<(), WebRTCError> {
        let peer = self
            ._get_subscriber_peer(target_id, participant_id)?
            .clone();

        let sdp_string = sdp.to_string();

        tokio::task::block_in_place(move || {
            let handle =
                tokio::runtime::Handle::try_current().map_err(|_| WebRTCError::FailedToSetSdp)?;

            handle.block_on(async move {
                let answer_desc = RTCSessionDescription::answer(sdp_string)
                    .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

                peer.set_remote_description(answer_desc)
                    .await
                    .map_err(|_| WebRTCError::FailedToSetSdp)
            })
        })
    }

    pub async fn handle_publisher_renegotiation(
        &self,
        participant_id: &str,
        sdp: &str,
    ) -> Result<String, WebRTCError> {
        let participant = self._get_publisher(participant_id)?;

        let peer = &participant.peer_connection;

        let offer_desc = RTCSessionDescription::offer(sdp.to_string())
            .map_err(|_| WebRTCError::FailedToCreateOffer)?;

        peer.set_remote_description(offer_desc)
            .await
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        let answer_desc = peer
            .create_answer(None)
            .await
            .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

        peer.set_local_description(answer_desc.clone())
            .await
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        Ok(answer_desc.clone().sdp)
    }

    pub async fn handle_migrate_connection(
        &self,
        participant_id: &str,
        sdp: &str,
        connection_type: ConnectionType,
    ) -> Result<Option<String>, WebRTCError> {
        let participant = self._get_publisher(participant_id)?;

        participant.set_connection_type(connection_type.clone());

        if connection_type == ConnectionType::SFU {
            let peer = &participant.peer_connection;

            let offer_desc = RTCSessionDescription::offer(sdp.to_string())
                .map_err(|_| WebRTCError::FailedToCreateOffer)?;

            peer.set_remote_description(offer_desc)
                .await
                .map_err(|_| WebRTCError::FailedToSetSdp)?;

            let answer_desc = peer
                .create_answer(None)
                .await
                .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

            peer.set_local_description(answer_desc.clone())
                .await
                .map_err(|_| WebRTCError::FailedToSetSdp)?;

            Ok(Some(answer_desc.clone().sdp))
        } else {
            let media = self._get_media(participant_id)?;

            let mut writer = media.write();

            writer.remove_all_tracks();

            writer.cache_sdp(sdp.to_owned());

            Ok(None)
        }
    }

    pub fn add_publisher_candidate(
        &self,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let participant = self._get_publisher(participant_id)?;
        let peer = &participant.peer_connection;

        let candidate_init = RTCIceCandidateInit {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_m_line_index,
            username_fragment: None,
        };

        // Clone peer và candidate để move vào async block
        let peer = peer.clone();
        let candidate_init = candidate_init.clone();

        tokio::task::block_in_place(move || {
            let handle = tokio::runtime::Handle::try_current()
                .map_err(|_| WebRTCError::FailedToAddCandidate)?;

            handle.block_on(async move {
                peer.add_ice_candidate(candidate_init)
                    .await
                    .map_err(|_| WebRTCError::FailedToAddCandidate)
            })
        })
    }

    pub fn add_subscriber_candidate(
        &self,
        target_id: &str,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let peer = self._get_subscriber_peer(target_id, participant_id)?;

        let candidate_init = RTCIceCandidateInit {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_m_line_index,
            username_fragment: None,
        };

        let peer = peer.clone();
        let candidate_init = candidate_init.clone();

        tokio::task::block_in_place(move || {
            let handle = tokio::runtime::Handle::try_current()
                .map_err(|_| WebRTCError::FailedToAddCandidate)?;

            handle.block_on(async move {
                peer.add_ice_candidate(candidate_init)
                    .await
                    .map_err(|_| WebRTCError::FailedToAddCandidate)
            })
        })
    }

    pub fn leave_room(&mut self, participant_id: &str) {
        self._remove_all_subscribers_with_target_id(participant_id);

        if let Some((_id, publisher)) = self.publishers.remove(participant_id) {
            publisher.close();
        }
    }

    pub fn set_e2ee_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let media = media.write();

        media.set_e2ee_enabled(is_enabled);

        Ok(())
    }

    pub fn set_camera_type(
        &self,
        participant_id: &str,
        camera_type: u8,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let media = media.write();

        media.set_camera_type(camera_type);

        Ok(())
    }

    pub fn set_video_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let media = media.write();

        let _ = media.set_video_enabled(is_enabled);

        Ok(())
    }

    pub fn set_audio_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let media = media.write();

        let _ = media.set_audio_enabled(is_enabled);

        Ok(())
    }

    pub fn set_screen_sharing(
        &self,
        participant_id: &str,
        is_enabled: bool,
        screen_track_id: Option<String>,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let media = media.write();

        let _ = media.set_screen_sharing(is_enabled, screen_track_id);

        Ok(())
    }

    pub fn set_hand_raising(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let media = media.write();

        let _ = media.set_hand_rasing(is_enabled);

        Ok(())
    }

    fn _get_publisher(&self, participant_id: &str) -> Result<Arc<Publisher>, WebRTCError> {
        let result = self
            .publishers
            .get(participant_id)
            .map(|r| r.clone())
            .ok_or_else(|| WebRTCError::ParticipantNotFound)?;

        Ok(result)
    }

    fn _add_publisher(&self, participant_id: &str, participant: &Arc<Publisher>) {
        self.publishers
            .insert(participant_id.to_owned(), participant.clone());
    }

    fn _add_subscriber(&self, peer_id: &str, pc: &Arc<RTCPeerConnection>, user_id: String) {
        let subscriber = Arc::new(Subscriber::new(pc.clone(), user_id));

        self.subscribers.insert(peer_id.to_owned(), subscriber);
    }

    fn _get_subscriber_peer(
        &self,
        target_id: &str,
        participant_id: &str,
    ) -> Result<Arc<RTCPeerConnection>, WebRTCError> {
        let key = self._get_subscriber_peer_id(target_id, participant_id);

        let subscribers = &self.subscribers;

        if let Some(subscriber) = subscribers.get(&key) {
            // Clone the peer_connection from subscriber
            Ok(Arc::clone(&subscriber.peer_connection))
        } else {
            Err(WebRTCError::PeerNotFound)
        }
    }

    fn _get_subscriber(
        &self,
        target_id: &str,
        participant_id: &str,
    ) -> Result<Arc<Subscriber>, WebRTCError> {
        let key = self._get_subscriber_peer_id(target_id, participant_id);

        let subscribers = &self.subscribers;

        if let Some(subscriber) = subscribers.get(&key) {
            // Clone the subscriber directly
            Ok(Arc::clone(&subscriber))
        } else {
            Err(WebRTCError::PeerNotFound)
        }
    }

    fn _get_subscriber_peer_id(&self, target_id: &str, participant_id: &str) -> String {
        let key = format!("p_{}_{}", target_id, participant_id);

        key
    }

    fn _get_media(&self, participant_id: &str) -> Result<Arc<RwLock<Media>>, WebRTCError> {
        let participant = self._get_publisher(participant_id)?;
        Ok(Arc::clone(&participant.media))
    }

    fn _remove_all_subscribers_with_target_id(&self, participant_id: &str) {
        let prefix = format!("p_{}_", participant_id);

        let subscribers = &self.subscribers;

        let keys_to_remove: Vec<String> = subscribers
            .iter()
            .filter(|entry| entry.key().starts_with(&prefix))
            .map(|entry| entry.key().clone())
            .collect();

        // Iterate through the keys to remove them
        for key in keys_to_remove {
            if let Some((_id, subscriber)) = subscribers.remove(&key) {
                let subscriber_clone: Arc<Subscriber> = Arc::clone(&subscriber);
                subscriber_clone.close();
            }
        }
    }

    async fn _add_track_to_subscribers(
        subscribers_lock: Arc<DashMap<String, Arc<Subscriber>>>,
        remote_track: TrackMutexWrapper,
        target_id: &str,
    ) -> Result<(), WebRTCError> {
        let prefix_track_id = format!("p_{}_", target_id);

        let peer_ids: Vec<String> = subscribers_lock
            .iter()
            .filter(|entry| entry.key().starts_with(&prefix_track_id))
            .map(|entry| entry.key().clone())
            .collect();

        for peer_id in peer_ids {
            if let Some(subscriber) = subscribers_lock.get(&peer_id) {
                let _ = subscriber.add_track(remote_track.clone()).await?;
            }
        }

        Ok(())
    }

    pub async fn _create_pc(&self) -> Result<Arc<RTCPeerConnection>, WebRTCError> {
        let config = RTCConfiguration {
            ice_servers: vec![],
            bundle_policy: RTCBundlePolicy::MaxBundle,
            rtcp_mux_policy: RTCRtcpMuxPolicy::Require,
            ice_transport_policy: RTCIceTransportPolicy::All,
            ice_candidate_pool_size: 20,
            ..Default::default()
        };

        let mut m = MediaEngine::default();
        let _ = m.register_default_codecs();

        let feedbacks = vec![
            RTCPFeedback {
                typ: TYPE_RTCP_FB_GOOG_REMB.to_owned(),
                parameter: "".to_string(),
            },
            RTCPFeedback {
                typ: TYPE_RTCP_FB_TRANSPORT_CC.to_owned(),
                parameter: "".to_string(),
            },
            RTCPFeedback {
                typ: TYPE_RTCP_FB_NACK.to_owned(),
                parameter: "".to_string(),
            },
        ];

        for fb in feedbacks {
            m.register_feedback(fb, RTPCodecType::Video);
        }

        // Enable Extension Headers needed for Simulcast
        for extension in [
            "urn:ietf:params:rtp-hdrext:sdes:mid",
            "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id",
            "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id",
            "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time",
        ] {
            m.register_header_extension(
                RTCRtpHeaderExtensionCapability {
                    uri: extension.to_owned(),
                },
                RTPCodecType::Video,
                None,
            )
            .ok();
        }

        let mut setting_engine = SettingEngine::default();
        setting_engine.set_lite(true);
        setting_engine.set_network_types(vec![NetworkType::Udp4]);
        setting_engine.set_udp_network(UDPNetwork::Ephemeral(
            EphemeralUDP::new(self.configs.port_min, self.configs.port_max).unwrap(),
        ));
        if !self.configs.public_ip.is_empty() {
            setting_engine.set_nat_1to1_ips(
                vec![self.configs.public_ip.to_owned()],
                RTCIceCandidateType::Host,
            );
        }

        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut m)
            .map_err(|_| WebRTCError::FailedToCreatePeer)?;

        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_setting_engine(setting_engine)
            .with_interceptor_registry(registry)
            .build();

        let peer = Arc::new(
            api.new_peer_connection(config)
                .await
                .map_err(|_| WebRTCError::FailedToCreatePeer)?,
        );

        Ok(peer)
    }

    async fn _extract_subscribe_response(
        &self,
        media_arc: &Arc<RwLock<Media>>,
    ) -> SubscribeResponse {
        let media = media_arc.read();
        let media_state = media.state.read();

        SubscribeResponse {
            camera_type: media_state.camera_type.clone(),
            video_enabled: media_state.video_enabled,
            audio_enabled: media_state.audio_enabled,
            is_hand_raising: media_state.is_hand_raising,
            is_e2ee_enabled: media_state.is_e2ee_enabled,
            is_screen_sharing: media_state.is_screen_sharing,
            screen_track_id: media_state.screen_track_id.clone(),
            video_codec: media_state.codec.clone(),
            offer: String::new(),
        }
    }

    async fn _forward_all_tracks(
        &self,
        subscriber: Arc<Subscriber>,
        media_arc: &Arc<RwLock<Media>>,
    ) -> Result<(), WebRTCError> {
        let media = media_arc.read();
        let tracks = media.tracks.clone();

        for entry in tracks.iter() {
            let _ = subscriber.add_track(Arc::clone(entry.value())).await?;
        }
        Ok(())
    }
}
