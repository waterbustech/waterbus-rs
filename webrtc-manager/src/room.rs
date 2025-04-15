use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::{sync::Mutex, time::sleep};
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
        setting_engine::SettingEngine,
    },
    ice::{
        network_type::NetworkType,
        udp_network::{EphemeralUDP, UDPNetwork},
    },
    ice_transport::{ice_candidate::RTCIceCandidateInit, ice_server::RTCIceServer},
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
        RTCPFeedback, TYPE_RTCP_FB_GOOG_REMB, TYPE_RTCP_FB_TRANSPORT_CC,
        rtp_codec::{RTCRtpHeaderExtensionCapability, RTPCodecType},
    },
};

use crate::{
    entities::{media::Media, publisher::Publisher, subscriber::Subscriber},
    errors::WebRTCError,
    models::{
        AddTrackResponse, IceCandidate, JoinRoomParams, JoinRoomResponse, SubscribeParams,
        SubscribeResponse, TrackMutexWrapper,
    },
};

#[derive(Debug, Clone)]
pub struct Room {
    publishers: Arc<Mutex<HashMap<String, Arc<Publisher>>>>,
    subscribers: Arc<Mutex<HashMap<String, Arc<Subscriber>>>>,
}

impl Room {
    pub fn new() -> Self {
        Self {
            publishers: Arc::new(Mutex::new(HashMap::new())),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn join_room(
        &mut self,
        params: JoinRoomParams,
        room_id: &str,
    ) -> Result<JoinRoomResponse, WebRTCError> {
        let participant_id = params.participant_id;

        let pc = self._create_pc().await?;

        let media = Media::new(
            participant_id.clone(),
            params.is_video_enabled,
            params.is_audio_enabled,
            params.is_e2ee_enabled,
        );

        let publisher = Arc::new(Publisher::new(Arc::new(Mutex::new(media)), pc.clone()));
        self._add_participant(&participant_id, &publisher).await;

        // === Peer Connection Callbacks ===
        let has_emitted = Arc::new(Mutex::new(false));
        {
            let peer_clone = pc.clone();
            let callback = params.callback.clone();
            let has_emitted = has_emitted.clone();

            pc.on_peer_connection_state_change(Box::new(move |_| {
                let peer = peer_clone.clone();
                let callback = callback.clone();
                let has_emitted = has_emitted.clone();

                Box::pin(async move {
                    if peer.connection_state() == RTCPeerConnectionState::Connected {
                        let mut emitted = has_emitted.lock().await;
                        if !*emitted {
                            *emitted = true;
                            tokio::spawn(async move {
                                sleep(Duration::from_secs(2)).await;
                                (callback)().await;
                            });
                        }
                    }
                })
            }));
        }
        // === Media Track ===
        {
            let media = self._get_media(&participant_id).await?;
            let room_id = room_id.to_string();
            let participant_id = participant_id.clone();
            let subscribers = Arc::clone(&self.subscribers);
            let publisher = publisher.clone();

            pc.on_track(Box::new(move |track, _, _| {
                let media = Arc::clone(&media);
                let subscribers = Arc::clone(&subscribers);
                let room_id = room_id.clone();
                let participant_id = participant_id.clone();

                publisher.send_rtcp_pli(track.ssrc());

                Box::pin(async move {
                    tokio::spawn(async move {
                        let mut media = media.lock().await;
                        let add_track_response = media.add_track(track, room_id).await;

                        match add_track_response {
                            AddTrackResponse::AddTrackSuccess(track) => {
                                let subscribers = subscribers.lock().await;
                                if let Err(e) = Self::_add_track_to_subscribers(
                                    subscribers,
                                    track,
                                    &participant_id,
                                )
                                .await
                                {
                                    eprintln!("Failed to add track to subscribers: {:?}", e);
                                }
                            }

                            AddTrackResponse::AddSimulcastTrackSuccess(track) => {
                                let subscribers = subscribers.lock().await;

                                if let Err(e) = Self::_add_track_to_subscribers(
                                    subscribers,
                                    track,
                                    &participant_id,
                                )
                                .await
                                {
                                    eprintln!("Failed to update track to subscribers: {:?}", e);
                                }
                            }

                            AddTrackResponse::FailedToAddTrack => {
                                eprintln!("Failed to add track");
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
        let sdp = RTCSessionDescription::offer(params.sdp.clone())
            .map_err(|_| WebRTCError::FailedToCreateOffer)?;

        pc.set_remote_description(sdp)
            .await
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        let answer = pc
            .create_answer(None)
            .await
            .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

        let mut gather_complete = pc.gathering_complete_promise().await;

        pc.set_local_description(answer)
            .await
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        let _ = gather_complete.recv().await;

        let local_desc = pc
            .local_description()
            .await
            .ok_or(WebRTCError::FailedToGetSdp)?;

        Ok(JoinRoomResponse {
            sdp: local_desc.sdp.clone(),
            is_recording: false,
        })
    }

    pub async fn subscribe(
        &mut self,
        params: SubscribeParams,
    ) -> Result<SubscribeResponse, WebRTCError> {
        let target_id = &params.target_id;
        let participant_id = &params.participant_id;

        let peer_id = self._get_subscriber_peer_id(target_id, participant_id);

        let pc = self._create_pc().await?;

        self._add_subscriber(&peer_id, &pc).await;

        let media_arc = self._get_media(target_id).await?;

        let subscribe_response = self._extract_subscribe_response(&media_arc).await;

        // Clone for callbacks
        let peer_clone = pc.clone();
        let media_clone = Arc::clone(&media_arc);
        let renegotiation_callback = params.on_negotiation_needed.clone();
        pc.on_negotiation_needed(Box::new(move || {
            let peer = peer_clone.clone();
            let media = media_clone.clone();
            let callback = renegotiation_callback.clone();
            Box::pin(async move {
                let media = media.lock().await;
                if media.tracks.lock().await.len() < 3 {
                    println!("Not enough tracks, skipping renegotiation");
                    return;
                }

                if let Ok(desc) = peer.create_offer(None).await {
                    let _ = peer.set_local_description(desc.clone()).await;
                    callback(desc.sdp);
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
                        eprintln!("Failed to convert ICE candidate");
                    }
                }
            })
        }));

        let subscriber = self._get_subscriber(target_id, participant_id).await?;
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

        Ok(SubscribeResponse {
            offer: local_desc.sdp.clone(),
            ..subscribe_response
        })
    }

    pub async fn set_subscriber_remote_sdp(
        &self,
        target_id: &str,
        participant_id: &str,
        sdp: &str,
    ) -> Result<(), WebRTCError> {
        let peer = self._get_subscriber_peer(target_id, participant_id).await?;

        let answer_desc = RTCSessionDescription::answer(sdp.to_string())
            .map_err(|_| WebRTCError::FailedToCreateAnswer)?;

        peer.set_remote_description(answer_desc)
            .await
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        Ok(())
    }

    pub async fn handle_publisher_renegotiation(
        &self,
        participant_id: &str,
        sdp: &str,
    ) -> Result<String, WebRTCError> {
        let participant = self._get_participant(participant_id).await?;

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

    pub async fn add_publisher_candidate(
        &self,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let participant = self._get_participant(participant_id).await?;

        let peer = &participant.peer_connection;

        let candidate = RTCIceCandidateInit {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_m_line_index,
            username_fragment: None,
        };

        peer.add_ice_candidate(candidate)
            .await
            .map_err(|_| WebRTCError::FailedToAddCandidate)?;

        Ok(())
    }

    pub async fn add_subscriber_candidate(
        &self,
        target_id: &str,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let peer = self._get_subscriber_peer(target_id, participant_id).await?;

        let candidate = RTCIceCandidateInit {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_m_line_index,
            username_fragment: None,
        };

        peer.add_ice_candidate(candidate)
            .await
            .map_err(|_| WebRTCError::FailedToAddCandidate)?;

        Ok(())
    }

    pub async fn leave_room(&mut self, participant_id: &str) {
        let mut participants = self.publishers.lock().await;

        self._remove_all_subscribers_with_target_id(participant_id)
            .await;

        if let Some(participant) = participants.get(participant_id) {
            participant.close().await;
        }

        participants.remove(participant_id);
    }

    pub async fn set_e2ee_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id).await?;

        let mut media = media.lock().await;

        media.set_e2ee_enabled(is_enabled);

        Ok(())
    }

    pub async fn set_camera_type(
        &self,
        participant_id: &str,
        camera_type: u8,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id).await?;

        let mut media = media.lock().await;

        media.set_camera_type(camera_type);

        Ok(())
    }

    pub async fn set_video_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id).await?;

        let mut media = media.lock().await;

        let _ = media.set_video_enabled(is_enabled);

        Ok(())
    }

    pub async fn set_audio_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id).await?;

        let mut media = media.lock().await;

        let _ = media.set_audio_enabled(is_enabled);

        Ok(())
    }

    pub async fn set_screen_sharing(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id).await?;

        let mut media = media.lock().await;

        let _ = media.set_screen_sharing(is_enabled).await;

        Ok(())
    }

    pub async fn set_hand_raising(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id).await?;

        let mut media = media.lock().await;

        let _ = media.set_hand_rasing(is_enabled);

        Ok(())
    }

    async fn _get_participant(&self, participant_id: &str) -> Result<Arc<Publisher>, WebRTCError> {
        let participants = self.publishers.lock().await;

        let result = participants
            .get(participant_id)
            .cloned()
            .ok_or_else(|| WebRTCError::ParticipantNotFound)?;

        Ok(result)
    }

    async fn _add_participant(&self, participant_id: &str, participant: &Arc<Publisher>) {
        let mut participants = self.publishers.lock().await;

        participants.insert(participant_id.to_owned(), participant.clone());
    }

    async fn _add_subscriber(&self, peer_id: &str, pc: &Arc<RTCPeerConnection>) {
        let mut subscribers = self.subscribers.lock().await;

        let subscriber = Arc::new(Subscriber::new(pc.clone()));

        subscribers.insert(peer_id.to_owned(), subscriber);
    }

    async fn _get_subscriber_peer(
        &self,
        target_id: &str,
        participant_id: &str,
    ) -> Result<Arc<RTCPeerConnection>, WebRTCError> {
        let key = self._get_subscriber_peer_id(target_id, participant_id);

        let subscribers = self.subscribers.lock().await;

        if let Some(subscriber) = subscribers.get(&key) {
            Ok(Arc::clone(&subscriber.peer_connection))
        } else {
            Err(WebRTCError::PeerNotFound)
        }
    }

    async fn _get_subscriber(
        &self,
        target_id: &str,
        participant_id: &str,
    ) -> Result<Arc<Subscriber>, WebRTCError> {
        let key = self._get_subscriber_peer_id(target_id, participant_id);

        let subscribers = self.subscribers.lock().await;

        if let Some(subscriber) = subscribers.get(&key) {
            Ok(Arc::clone(&subscriber))
        } else {
            Err(WebRTCError::PeerNotFound)
        }
    }

    fn _get_subscriber_peer_id(&self, target_id: &str, participant_id: &str) -> String {
        let key = format!("p_{}_{}", target_id, participant_id);

        key
    }

    async fn _get_media(&self, participant_id: &str) -> Result<Arc<Mutex<Media>>, WebRTCError> {
        let participant = self._get_participant(participant_id).await?;
        Ok(Arc::clone(&participant.media))
    }

    async fn _remove_all_subscribers_with_target_id(&self, participant_id: &str) {
        let prefix = format!("p_{}_", participant_id);

        let mut subscribers = self.subscribers.lock().await;

        let keys_to_remove: Vec<String> = subscribers
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();

        for key in keys_to_remove {
            if let Some(subscriber) = subscribers.remove(&key) {
                let subscriber_clone = Arc::clone(&subscriber);
                tokio::spawn(async move {
                    let _ = subscriber_clone.close().await;
                });
            }
        }
    }

    async fn _add_track_to_subscribers(
        subscribers_lock: tokio::sync::MutexGuard<'_, HashMap<String, Arc<Subscriber>>>,
        remote_track: TrackMutexWrapper,
        target_id: &str,
    ) -> Result<(), WebRTCError> {
        let prefix_track_id = format!("p_{}_", target_id);

        let peer_ids: Vec<String> = subscribers_lock
            .keys()
            .filter(|track_id| track_id.starts_with(&prefix_track_id))
            .cloned()
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
            ice_servers: self._get_ice_servers(),
            bundle_policy: RTCBundlePolicy::MaxBundle,
            rtcp_mux_policy: RTCRtcpMuxPolicy::Require,
            ice_transport_policy: RTCIceTransportPolicy::All,
            ice_candidate_pool_size: 20,
            ..Default::default()
        };

        let mut m = MediaEngine::default();
        let _ = m.register_default_codecs();

        m.register_feedback(
            RTCPFeedback {
                typ: TYPE_RTCP_FB_GOOG_REMB.to_owned(),
                parameter: "".to_owned(),
            },
            RTPCodecType::Video,
        );
        m.register_feedback(
            RTCPFeedback {
                typ: TYPE_RTCP_FB_TRANSPORT_CC.to_owned(),
                parameter: "".to_owned(),
            },
            RTPCodecType::Video,
        );

        // Enable Extension Headers needed for Simulcast
        for extension in [
            "urn:ietf:params:rtp-hdrext:sdes:mid",
            "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id",
            "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id",
            "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01",
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
        setting_engine.set_network_types(vec![NetworkType::Udp4]);
        setting_engine.set_udp_network(UDPNetwork::Ephemeral(
            EphemeralUDP::new(10000, 60000).unwrap(),
        ));

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
        media_arc: &Arc<Mutex<Media>>,
    ) -> SubscribeResponse {
        let media = media_arc.lock().await;
        SubscribeResponse {
            camera_type: media.camera_type.clone(),
            video_enabled: media.video_enabled,
            audio_enabled: media.audio_enabled,
            is_hand_raising: media.is_hand_raising,
            is_e2ee_enabled: media.is_e2ee_enabled,
            is_screen_sharing: media.is_screen_sharing,
            video_codec: media.codec.clone(),
            offer: String::new(),
        }
    }

    async fn _forward_all_tracks(
        &self,
        subscriber: Arc<Subscriber>,
        media_arc: &Arc<Mutex<Media>>,
    ) -> Result<(), WebRTCError> {
        let media = media_arc.lock().await;
        let tracks = media.tracks.lock().await;

        for track_wrapper in tracks.iter() {
            let _ = subscriber.add_track(Arc::clone(track_wrapper)).await?;
        }
        Ok(())
    }

    pub fn _get_ice_servers(&self) -> Vec<RTCIceServer> {
        use webrtc::ice_transport::ice_server::RTCIceServer;

        let ice_servers = vec![
            RTCIceServer {
                urls: vec!["stun:turn.waterbus.tech:3478".to_string()],
                ..Default::default()
            },
            RTCIceServer {
                urls: vec!["turn:turn.waterbus.tech:3478?transport=udp".to_string()],
                username: "waterbus".to_string(),
                credential: "waterbus".to_string(),
                ..Default::default()
            },
        ];

        ice_servers
    }
}
