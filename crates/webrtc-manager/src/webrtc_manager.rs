use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;

use crate::{
    errors::WebRTCError,
    models::{
        connection_type::ConnectionType,
        params::{
            IceCandidate, IceCandidateCallback, JoinRoomParams, JoinRoomResponse, JoinedCallback,
            RenegotiationCallback, SubscribeHlsLiveStreamParams, SubscribeHlsLiveStreamResponse,
            SubscribeParams, SubscribeResponse, WClient, WebRTCManagerConfigs,
        },
    },
    room::Room,
};

pub struct JoinRoomReq {
    pub client_id: String,
    pub participant_id: String,
    pub room_id: String,
    pub sdp: String,
    pub is_video_enabled: bool,
    pub is_audio_enabled: bool,
    pub is_e2ee_enabled: bool,
    pub total_tracks: u8,
    pub connection_type: u8,
    pub callback: JoinedCallback,
    pub ice_candidate_callback: IceCandidateCallback,
    pub streaming_protocol: u8,
    pub is_ipv6_supported: bool,
}

#[derive(Clone)]
pub struct WebRTCManager {
    rooms: Arc<DashMap<String, Arc<RwLock<Room>>>>,
    clients: Arc<DashMap<String, WClient>>,
    configs: WebRTCManagerConfigs,
}

impl WebRTCManager {
    pub fn new(configs: WebRTCManagerConfigs) -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
            clients: Arc::new(DashMap::new()),
            configs,
        }
    }

    #[allow(clippy::all)]
    pub async fn join_room(
        &self,
        req: JoinRoomReq,
    ) -> Result<Option<JoinRoomResponse>, WebRTCError> {
        let client_id = &req.client_id;
        let room_id = &req.room_id;
        let participant_id = &req.participant_id;

        self._add_client(
            client_id,
            WClient {
                participant_id: participant_id.clone(),
                room_id: room_id.clone(),
            },
        );

        let room = {
            let room_result = self._get_room_by_id(room_id);
            match room_result {
                Ok(room) => room,
                Err(_) => self._add_room(room_id)?,
            }
        };

        let params = JoinRoomParams {
            participant_id: participant_id.to_string(),
            sdp: req.sdp,
            is_audio_enabled: req.is_audio_enabled,
            is_video_enabled: req.is_video_enabled,
            is_e2ee_enabled: req.is_e2ee_enabled,
            total_tracks: req.total_tracks,
            connection_type: ConnectionType::from(req.connection_type),
            callback: req.callback,
            on_candidate: req.ice_candidate_callback,
            streaming_protocol: req.streaming_protocol.into(),
            is_ipv6_supported: req.is_ipv6_supported,
        };

        let res = {
            let mut room = room.write();
            room.join_room(params, room_id).await?
        };

        Ok(res)
    }

    #[allow(clippy::all)]
    pub async fn subscribe(
        &self,
        client_id: &str,
        target_id: &str,
        participant_id: &str,
        room_id: &str,
        renegotiation_callback: RenegotiationCallback,
        ice_candidate_callback: IceCandidateCallback,
        is_ipv6_supported: bool,
    ) -> Result<SubscribeResponse, WebRTCError> {
        self._add_client(
            client_id,
            WClient {
                participant_id: participant_id.to_owned(),
                room_id: room_id.to_owned(),
            },
        );

        let room = self._get_room_by_id(room_id)?;
        let mut room = room.write();

        let params = SubscribeParams {
            participant_id: participant_id.to_string(),
            target_id: (&target_id).to_string(),
            on_candidate: ice_candidate_callback,
            on_negotiation_needed: renegotiation_callback,
            is_ipv6_supported,
        };

        let res = room.subscribe(params).await?;

        Ok(res)
    }

    pub fn subscribe_hls_live_stream(
        &self,
        client_id: &str,
        target_id: &str,
    ) -> Result<SubscribeHlsLiveStreamResponse, WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.read();

        let res = room.subscribe_hls_live_stream(SubscribeHlsLiveStreamParams {
            target_id: target_id.to_string(),
            participant_id: participant_id.to_string(),
        })?;

        Ok(res)
    }

    #[allow(clippy::all)]
    pub fn set_subscriber_desc(
        &self,
        client_id: &str,
        target_id: &str,
        sdp: &str,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.read();

        room.set_subscriber_remote_sdp(target_id, participant_id, sdp)?;

        Ok(())
    }

    #[allow(clippy::all)]
    pub async fn handle_publisher_renegotiation(
        &self,
        client_id: &str,
        sdp: &str,
    ) -> Result<String, WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.read();

        let sdp = room
            .handle_publisher_renegotiation(participant_id, sdp)
            .await?;

        Ok(sdp)
    }

    #[allow(clippy::all)]
    pub async fn handle_migrate_connection(
        &self,
        client_id: &str,
        sdp: &str,
        connection_type: ConnectionType,
    ) -> Result<Option<String>, WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.read();

        let sdp = room
            .handle_migrate_connection(participant_id, sdp, connection_type)
            .await?;

        Ok(sdp)
    }

    pub fn add_publisher_candidate(
        &self,
        client_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.read();

        room.add_publisher_candidate(participant_id, candidate)?;

        Ok(())
    }

    pub fn add_subscriber_candidate(
        &self,
        client_id: &str,
        target_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.read();

        room.add_subscriber_candidate(target_id, participant_id, candidate)?;

        Ok(())
    }

    pub fn leave_room(&self, client_id: &str) -> Result<WClient, WebRTCError> {
        let client = self.get_client_by_id(client_id)?.clone();
        let room_id = &client.room_id;
        let participant_id = client.participant_id.clone();

        let room = self._get_room_by_id(room_id)?;

        let mut room_clone_for_leave = {
            let room_guard = room.read();
            room_guard.clone()
        };

        room_clone_for_leave.leave_room(&participant_id);

        self._remove_client(client_id);

        Ok(client)
    }

    pub fn set_audio_enabled(&self, client_id: &str, is_enabled: bool) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_audio_enabled(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_video_enabled(&self, client_id: &str, is_enabled: bool) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_video_enabled(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_camera_type(&self, client_id: &str, camera_type: u8) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_camera_type(&participant_id, camera_type)?;

        Ok(())
    }

    pub fn set_e2ee_enabled(&self, client_id: &str, is_enabled: bool) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_e2ee_enabled(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_screen_sharing(
        &self,
        client_id: &str,
        is_enabled: bool,
        screen_track_id: Option<String>,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_screen_sharing(&participant_id, is_enabled, screen_track_id)?;

        Ok(())
    }

    pub fn set_hand_raising(&self, client_id: &str, is_enabled: bool) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_hand_raising(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn _add_client(&self, client_id: &str, info: WClient) {
        if !self.clients.contains_key(client_id) {
            self.clients.insert(client_id.to_string(), info);
        }
    }

    pub fn _remove_client(&self, client_id: &str) {
        self.clients.remove(client_id);
    }

    fn _add_room(&self, room_id: &str) -> Result<Arc<RwLock<Room>>, WebRTCError> {
        let room_value = Arc::new(RwLock::new(Room::new(self.configs.clone())));

        self.rooms
            .insert(room_id.to_string(), Arc::clone(&room_value));

        Ok(room_value)
    }

    pub fn get_client_by_id(&self, client_id: &str) -> Result<WClient, WebRTCError> {
        if let Some(client) = self.clients.get(client_id) {
            Ok(client.clone())
        } else {
            Err(WebRTCError::ParticipantNotFound)
        }
    }

    pub fn _get_room_by_id(&self, room_id: &str) -> Result<Arc<RwLock<Room>>, WebRTCError> {
        if let Some(room) = self.rooms.get(room_id) {
            Ok(room.clone())
        } else {
            Err(WebRTCError::RoomNotFound)
        }
    }
}
