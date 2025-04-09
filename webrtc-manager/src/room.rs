#![allow(unused)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use webrtc::{
    api,
    ice_transport::ice_candidate::RTCIceCandidateInit,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::{
        RTCRtpTransceiverInit,
        rtp_codec::{RTCRtpCodecCapability, RTPCodecType},
        rtp_sender::RTCRtpSender,
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
    },
    track::{
        track_local::{TrackLocal, track_local_static_rtp::TrackLocalStaticRTP},
        track_remote::TrackRemote,
    },
};

use crate::{
    entities::{media::Media, participant::Participant, track::Track},
    errors::WebRTCError,
    models::{IceCandidate, JoinRoomParams, JoinRoomResponse, SubscribeParams, SubscribeResponse},
};

pub struct Room {
    participants: HashMap<String, Arc<Participant>>,
    subscribers: HashMap<String, Arc<RTCPeerConnection>>,
}

impl Room {
    pub fn new() -> Self {
        Self {
            participants: HashMap::new(),
            subscribers: HashMap::new(),
        }
    }

    pub async fn join_room(
        &mut self,
        params: JoinRoomParams,
        room_id: &str,
    ) -> Result<JoinRoomResponse, WebRTCError> {
        let mut has_emit_new_participant_joined = false;
        let participant_id = params.participant_id;
        let room_id = room_id;

        let config = RTCConfiguration {
            ice_servers: vec![/* fill this in */],
            ..Default::default()
        };

        let peer = Arc::new(
            api::APIBuilder::new()
                .build()
                .new_peer_connection(config)
                .await
                .map_err(|err| WebRTCError::FailedToCreatePeer)?,
        );

        peer.add_transceiver_from_kind(RTPCodecType::Video, None)
            .await
            .map_err(|err| WebRTCError::FailedToAddTransceiver)?;
        peer.add_transceiver_from_kind(RTPCodecType::Audio, None)
            .await
            .map_err(|err| WebRTCError::FailedToAddTransceiver)?;

        let media = Media::new(
            participant_id.clone(),
            params.is_video_enabled,
            params.is_audio_enabled,
            params.is_e2ee_enabled,
        );

        let participant = Arc::new(Participant {
            media: Arc::new(Mutex::new(media)),
            peer_connection: peer.clone(),
        });

        self.participants
            .insert(participant_id.clone(), participant.clone());

        // Peer connection state change
        let has_emitted = Arc::new(Mutex::new(false));
        let callback = params.callback.clone();
        let peer_clone = peer.clone();

        peer.on_peer_connection_state_change(Box::new(move |_| {
            let peer = peer_clone.clone();
            let has_emitted = has_emitted.clone();
            let callback = callback.clone();

            Box::pin(async move {
                let state = peer.connection_state();
                if state == RTCPeerConnectionState::Connected {
                    let mut emitted = has_emitted.lock().unwrap();
                    if !*emitted {
                        *emitted = true;
                        (callback)();
                    }
                }
            })
        }));

        let media = self._get_media(&participant_id)?;
        let room_id_clone = room_id.to_string();
        let participant_id_clone = participant_id.to_string();
        let subscribers = Arc::new(self.subscribers.clone());

        peer.on_track(Box::new(move |track, _, _| {
            let media = Arc::clone(&media);
            let subscribers = Arc::clone(&subscribers);
            let room_id = room_id_clone.clone();
            let participant_id = participant_id_clone.clone();
            let participant = participant.clone();
            let track2 = Arc::clone(&track);

            // Send a PLI on an interval so that the publisher is pushing a keyframe
            let media_ssrc = track.ssrc();
            participant.send_rtcp_pli(media_ssrc);

            // Lock media inside the async block
            let mut media_guard = media.lock().unwrap();
            media_guard.add_track(track, room_id);

            tokio::spawn(async move {
                Self::_add_track_to_subscribers(subscribers, track2, &participant_id)
                    .await
                    .unwrap();
            });

            Box::pin(async move {})
        }));

        let sdp = RTCSessionDescription::offer(params.sdp.clone());

        match sdp {
            Ok(sdp) => {
                peer.set_remote_description(sdp)
                    .await
                    .map_err(|err| WebRTCError::FailedToSetSdp)?;

                let answer = peer
                    .create_answer(None)
                    .await
                    .map_err(|err| WebRTCError::FailedToCreateAnswer)?;
                peer.set_local_description(answer)
                    .await
                    .map_err(|err| WebRTCError::FailedToSetSdp)?;

                let local_desc = peer
                    .local_description()
                    .await
                    .ok_or_else(|| WebRTCError::FailedToGetSdp)?;

                return Ok(JoinRoomResponse {
                    offer: local_desc.sdp.clone(),
                    is_recording: false,
                });
            }
            Err(_) => Err(WebRTCError::FailedToCreateOffer),
        }
    }

    pub async fn subscribe(
        &mut self,
        params: SubscribeParams,
    ) -> Result<SubscribeResponse, WebRTCError> {
        let target_id = &params.target_id;
        let participant_id = &params.participant_id;

        let peer_id = self._get_subscriber_peer_id(target_id, participant_id);

        let config = RTCConfiguration {
            ice_servers: vec![/* fill this in */],
            ..Default::default()
        };

        let peer = Arc::new(
            api::APIBuilder::new()
                .build()
                .new_peer_connection(config)
                .await
                .map_err(|err| WebRTCError::FailedToCreatePeer)?,
        );

        self.subscribers.insert(peer_id, peer.clone());

        let media = self._get_media(target_id)?;
        let media = media.lock().unwrap();

        // Extract values before anything else
        let camera_type = media.camera_type.clone();
        let video_enabled = media.video_enabled;
        let audio_enabled = media.audio_enabled;
        let is_hand_raising = media.is_hand_raising;
        let is_e2ee_enabled = media.is_e2ee_enabled;
        let is_screen_sharing = media.is_screen_sharing;
        let video_codec = media.codec.clone();

        peer.on_negotiation_needed(Box::new(move || {
            Box::pin(async move {
                println!("Negotiation needed triggered");
            })
        }));

        peer.on_ice_candidate(Box::new(move |candidate| {
            let on_candidate = params.on_candidate.clone();

            Box::pin(async move {
                if let Some(candidate) = candidate {
                    (on_candidate)(candidate);
                }
            })
        }));

        let tracks = media.tracks.lock().unwrap();
        for track_wrapper in tracks.iter() {
            let remote_track = &track_wrapper.track;

            Self::_add_track(
                &peer,
                &remote_track.codec().capability.mime_type,
                &remote_track.id(),
                RTCRtpTransceiverDirection::Sendonly,
            );
        }

        let offer_desc = peer
            .create_offer(None)
            .await
            .map_err(|_| WebRTCError::FailedToCreateOffer)?;

        peer.set_local_description(offer_desc)
            .await
            .map_err(|_| WebRTCError::FailedToSetSdp)?;

        let local_desc = peer
            .local_description()
            .await
            .ok_or_else(|| WebRTCError::FailedToGetSdp)?;

        let subscribe_response = SubscribeResponse {
            offer: local_desc.sdp.clone(),
            camera_type,
            video_enabled,
            audio_enabled,
            is_hand_raising,
            is_e2ee_enabled,
            is_screen_sharing,
            video_codec,
        };

        Ok(subscribe_response)
    }

    pub async fn set_subscriber_remote_sdp(
        &self,
        target_id: &str,
        participant_id: &str,
        sdp: &str,
    ) -> Result<(), WebRTCError> {
        let peer = self._get_subscriber_peer(target_id, participant_id)?;

        let answer_desc = RTCSessionDescription::answer(sdp.to_string())
            .map_err(|err| WebRTCError::FailedToCreateAnswer)?;

        peer.set_remote_description(answer_desc)
            .await
            .map_err(|err| WebRTCError::FailedToSetSdp);

        Ok(())
    }

    pub async fn handle_publisher_renegotiation(
        &self,
        participant_id: &str,
        sdp: &str,
    ) -> Result<String, WebRTCError> {
        if let Some(participant) = self.participants.get(participant_id) {
            let peer = &participant.peer_connection;

            let offer_desc = RTCSessionDescription::offer(sdp.to_string())
                .map_err(|err| WebRTCError::FailedToCreateOffer)?;

            peer.set_local_description(offer_desc)
                .await
                .map_err(|err| WebRTCError::FailedToSetSdp);

            let answer_desc = peer
                .create_answer(None)
                .await
                .map_err(|err| WebRTCError::FailedToCreateAnswer)?;

            peer.set_local_description(answer_desc)
                .await
                .map_err(|_| WebRTCError::FailedToSetSdp)?;

            let local_desc = peer
                .local_description()
                .await
                .ok_or_else(|| WebRTCError::FailedToGetSdp)?;

            Ok((local_desc.sdp.clone()))
        } else {
            Err(WebRTCError::PeerNotFound)
        }
    }

    pub async fn add_publisher_candidate(
        &self,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        if let Some(participant) = self.participants.get(participant_id) {
            let peer = &participant.peer_connection;

            let candidate = RTCIceCandidateInit {
                candidate: candidate.candidate,
                sdp_mid: candidate.sdp_mid,
                sdp_mline_index: candidate.sdp_m_line_index,
                username_fragment: None,
            };

            peer.add_ice_candidate(candidate)
                .await
                .map_err(|err| WebRTCError::FailedToAddCandidate)?;

            Ok(())
        } else {
            Err(WebRTCError::PeerNotFound)
        }
    }

    pub async fn add_subscriber_candidate(
        &self,
        target_id: &str,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let peer = self._get_subscriber_peer(target_id, participant_id)?;

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
        self._remove_all_subscribers_with_target_id(participant_id);

        if let Some(participant) = self.participants.get(participant_id) {
            let media = participant.media.lock().unwrap();

            media.stop();
            participant.peer_connection.close();
        }

        self.participants.remove(participant_id);
    }

    pub fn set_e2ee_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let mut media = media.lock().unwrap();

        media.set_e2ee_enabled(is_enabled);

        Ok(())
    }

    pub fn set_camera_type(
        &self,
        participant_id: &str,
        camera_type: u8,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let mut media = media.lock().unwrap();

        media.set_camera_type(camera_type);

        Ok(())
    }

    pub fn set_video_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let mut media = media.lock().unwrap();

        Ok(())
    }

    pub fn set_audio_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let mut media = media.lock().unwrap();

        Ok(())
    }

    pub fn set_screen_sharing(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let mut media = media.lock().unwrap();

        media.set_screen_sharing(is_enabled);

        Ok(())
    }

    pub fn set_hand_raising(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let media = self._get_media(participant_id)?;

        let mut media = media.lock().unwrap();

        Ok(())
    }

    fn _get_other_participants(&self, participant_id: &str) -> Vec<Arc<Participant>> {
        self.participants
            .iter()
            .filter_map(|(id, participant)| {
                if id != participant_id {
                    Some(participant.clone()) // Clone the Arc
                } else {
                    None
                }
            })
            .collect()
    }

    fn _get_participant(&self, participant_id: &str) -> Result<&Arc<Participant>, WebRTCError> {
        let result = self
            .participants
            .get(participant_id)
            .ok_or_else(|| WebRTCError::ParticipantNotFound)?;

        Ok(result)
    }

    fn _get_subscriber_peer(
        &self,
        target_id: &str,
        participant_id: &str,
    ) -> Result<&RTCPeerConnection, WebRTCError> {
        let key = self._get_subscriber_peer_id(target_id, participant_id);

        if let Some(peer) = self.subscribers.get(&key) {
            Ok(peer)
        } else {
            Err(WebRTCError::PeerNotFound)
        }
    }

    fn _get_subscriber_peer_id(&self, target_id: &str, participant_id: &str) -> String {
        let key = format!("p_{}_{}", target_id, participant_id);

        key
    }

    fn _get_media(&self, participant_id: &str) -> Result<Arc<Mutex<Media>>, WebRTCError> {
        let participant = self._get_participant(participant_id)?;
        Ok(Arc::clone(&participant.media))
    }

    fn _remove_all_subscribers_with_target_id(&mut self, participant_id: &str) {
        let prefix = format!("p_{}", participant_id);
        let keys_to_remove: Vec<String> = self
            .subscribers
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();

        for key in keys_to_remove {
            if let Some(pc) = self.subscribers.remove(&key) {
                let pc_clone = Arc::clone(&pc);
                tokio::spawn(async move {
                    let _ = pc_clone.close().await;
                });
            }

            self.subscribers.remove(&key);
        }
    }

    async fn _add_track_to_subscribers(
        subscribers: Arc<HashMap<String, Arc<RTCPeerConnection>>>,
        track: Arc<TrackRemote>,
        target_id: &str,
    ) -> Result<(), WebRTCError> {
        let prefix_track_id = format!("p_{}_", target_id);

        let peer_ids: Vec<String> = subscribers
            .keys()
            .filter(|track_id| track_id.starts_with(&prefix_track_id))
            .cloned()
            .collect();

        let mime_type = track.codec().capability.mime_type.clone();
        let track_id = track.id().to_string();

        for peer_id in peer_ids {
            if let Some(pc) = subscribers.get(&peer_id) {
                Self::_add_track(
                    pc,
                    &mime_type,
                    &track_id,
                    RTCRtpTransceiverDirection::Sendonly,
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn _add_track(
        peer_connection: &Arc<RTCPeerConnection>,
        mime_type: &str,
        track_id: &str,
        direction: RTCRtpTransceiverDirection,
    ) -> Result<(Arc<RTCRtpSender>, Arc<TrackLocalStaticRTP>), WebRTCError> {
        // Create a video track
        let track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: mime_type.to_owned(),
                ..Default::default()
            },
            track_id.to_owned(),
            "webrtc-rs".to_owned(),
        ));

        // Add this newly created track to the PeerConnection
        let rtp_transceiver = peer_connection
            .add_transceiver_from_track(
                Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>,
                Some(RTCRtpTransceiverInit {
                    direction,
                    send_encodings: vec![],
                }),
            )
            .await
            .map_err(|_| WebRTCError::FailedToAddTrack)?;

        Ok((rtp_transceiver.sender().await, track))
    }
}
