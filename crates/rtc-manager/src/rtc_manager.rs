use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;

use crate::models::callbacks::{IceCandidateHandler, JoinedHandler, RenegotiationHandler};
use crate::models::rtc_dto::{JoinRoomParameters, SubscribeParameters};
use crate::utils::udp_runtime::RtcUdpRuntime;
use crate::{
    entities::room::Room,
    errors::RtcError,
    models::{
        connection_type::ConnectionType,
        rtc_dto::{
            IceCandidate, JoinRoomResponse, RtcManagerConfig, SubscribeHlsLiveStreamParams,
            SubscribeHlsLiveStreamResponse, SubscribeResponse, WClient,
        },
    },
};

#[derive(Clone)]
pub struct RtcManager {
    rooms: Arc<DashMap<String, Arc<RwLock<Room>>>>,
    clients: Arc<DashMap<String, WClient>>,
}

impl RtcManager {
    pub fn new(config: RtcManagerConfig) -> Self {
        // Initialize global UDP runtime once
        let _ = RtcUdpRuntime::init(config.clone());

        Self {
            rooms: Arc::new(DashMap::new()),
            clients: Arc::new(DashMap::new()),
        }
    }

    pub fn join_room<I, J>(
        &self,
        req: JoinRoomParameters<I, J>,
    ) -> Result<Option<JoinRoomResponse>, RtcError>
    where
        I: IceCandidateHandler,
        J: JoinedHandler + Clone,
    {
        let client_id = &req.client_id;
        let room_id = &req.room_id.clone();
        let participant_id = &req.participant_id.clone();

        self.add_client(
            client_id,
            WClient {
                participant_id: participant_id.clone(),
                room_id: room_id.clone(),
            },
        );

        let room = {
            let room_result = self.get_room_by_id(room_id);
            match room_result {
                Ok(room) => room,
                Err(_) => self.add_room(room_id)?,
            }
        };

        let res = {
            let mut room = room.write();
            room.join_room(req)?
        };

        Ok(res)
    }

    pub fn subscribe<I, R>(
        &self,
        req: SubscribeParameters<I, R>,
    ) -> Result<SubscribeResponse, RtcError>
    where
        I: IceCandidateHandler,
        R: RenegotiationHandler,
    {
        let client_id = &req.client_id;
        let room_id = &req.room_id;
        let participant_id = &req.participant_id;

        self.add_client(
            client_id,
            WClient {
                participant_id: participant_id.to_owned(),
                room_id: room_id.to_owned(),
            },
        );

        let room = self.get_room_by_id(room_id)?;
        let mut room = room.write();

        let res = room.subscribe(req)?;

        Ok(res)
    }

    pub fn subscribe_hls_live_stream(
        &self,
        client_id: &str,
        target_id: &str,
        participant_id: &str,
        room_id: &str,
    ) -> Result<SubscribeHlsLiveStreamResponse, RtcError> {
        self.add_client(
            client_id,
            WClient {
                participant_id: participant_id.to_owned(),
                room_id: room_id.to_owned(),
            },
        );

        let room = self.get_room_by_id(room_id)?;
        let room = room.read();

        let params = SubscribeHlsLiveStreamParams {
            target_id: target_id.to_string(),
            participant_id: participant_id.to_string(),
        };

        let res = room.subscribe_hls_live_stream(params)?;

        Ok(res)
    }

    pub fn add_publisher_candidate(
        &self,
        client_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.add_publisher_candidate(&participant_id, candidate)?;

        Ok(())
    }

    pub fn add_subscriber_candidate(
        &self,
        client_id: &str,
        target_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.add_subscriber_candidate(&target_id, &participant_id, candidate)?;

        Ok(())
    }

    pub fn set_subscriber_sdp(
        &self,
        client_id: &str,
        target_id: &str,
        sdp: String,
    ) -> Result<(), RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_subscriber_sdp(&target_id, &participant_id, sdp)?;

        Ok(())
    }

    pub fn publisher_renegotiation(
        &self,
        client_id: &str,
        sdp: String,
    ) -> Result<String, RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.publisher_renegotiation(&participant_id, sdp)
    }

    pub fn migrate_connection(
        &self,
        client_id: &str,
        sdp: String,
        connection_type: ConnectionType,
    ) -> Result<String, RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.migrate_connection(&participant_id, sdp, connection_type)
    }

    pub fn leave_room(&self, client_id: &str) -> Result<WClient, RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let mut room = room.write();

        room.leave_room(&participant_id);
        self.remove_client(client_id);

        Ok(client)
    }

    pub fn set_e2ee_enabled(&self, client_id: &str, is_enabled: bool) -> Result<(), RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_e2ee_enabled(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_camera_type(&self, client_id: &str, camera_type: u8) -> Result<(), RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_camera_type(&participant_id, camera_type)?;

        Ok(())
    }

    pub fn set_video_enabled(&self, client_id: &str, is_enabled: bool) -> Result<(), RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_video_enabled(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_audio_enabled(&self, client_id: &str, is_enabled: bool) -> Result<(), RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_audio_enabled(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_screen_sharing(
        &self,
        client_id: &str,
        is_sharing: bool,
        screen_track_id: Option<String>,
    ) -> Result<(), RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_screen_sharing(&participant_id, is_sharing, screen_track_id)?;

        Ok(())
    }

    pub fn set_hand_raising(&self, client_id: &str, is_enabled: bool) -> Result<(), RtcError> {
        let client = self.get_client_by_id(client_id)?;
        let room_id = client.room_id.clone();
        let participant_id = client.participant_id.clone();

        let room = self.get_room_by_id(&room_id)?;
        let room = room.read();

        room.set_hand_raising(&participant_id, is_enabled)?;

        Ok(())
    }

    fn add_client(&self, client_id: &str, info: WClient) {
        if !self.clients.contains_key(client_id) {
            self.clients.insert(client_id.to_string(), info);
        }
    }

    fn remove_client(&self, client_id: &str) {
        self.clients.remove(client_id);
    }

    fn get_client_by_id(&self, client_id: &str) -> Result<WClient, RtcError> {
        self.clients
            .get(client_id)
            .map(|entry| entry.value().clone())
            .ok_or(RtcError::PeerNotFound)
    }

    fn add_room(&self, room_id: &str) -> Result<Arc<RwLock<Room>>, RtcError> {
        let room = Arc::new(RwLock::new(Room::new(room_id.to_string())));
        self.rooms.insert(room_id.to_string(), room.clone());
        Ok(room)
    }

    fn get_room_by_id(&self, room_id: &str) -> Result<Arc<RwLock<Room>>, RtcError> {
        self.rooms
            .get(room_id)
            .map(|entry| Arc::clone(entry.value()))
            .ok_or(RtcError::RoomNotFound)
    }

    pub fn get_room_count(&self) -> usize {
        self.rooms.len()
    }

    pub fn get_client_count(&self) -> usize {
        self.clients.len()
    }
}
