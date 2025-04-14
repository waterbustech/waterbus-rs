use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;

use crate::{
    errors::WebRTCError,
    models::{
        IceCandidate, IceCandidateCallback, JoinRoomParams, JoinRoomResponse, JoinedCallback,
        RenegotiationCallback, SubscribeParams, SubscribeResponse, WClient,
    },
    room::Room,
};

pub struct JoinRoomReq {
    pub sdp: String,
    pub is_video_enabled: bool,
    pub is_audio_enabled: bool,
    pub is_e2ee_enabled: bool,
    pub callback: JoinedCallback,
    pub ice_candidate_callback: IceCandidateCallback,
}

#[derive(Debug, Clone)]
pub struct WebRTCManager {
    rooms: Arc<Mutex<HashMap<String, Arc<Mutex<Room>>>>>,
    clients: Arc<Mutex<HashMap<String, WClient>>>,
}

impl WebRTCManager {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(Mutex::new(HashMap::new())),
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn join_room(
        self,
        client_id: &str,
        req: JoinRoomReq,
    ) -> Result<JoinRoomResponse, WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = {
            let room_result = self._get_room_by_id(room_id).await;
            match room_result {
                Ok(room) => room,
                Err(_) => self._add_room(room_id).await?,
            }
        };

        let params = JoinRoomParams {
            participant_id: participant_id.to_string(),
            sdp: req.sdp,
            is_audio_enabled: req.is_audio_enabled,
            is_video_enabled: req.is_video_enabled,
            is_e2ee_enabled: req.is_e2ee_enabled,
            callback: req.callback,
            on_candidate: req.ice_candidate_callback,
        };

        let mut room = room.lock().await;
        let res = room.join_room(params, &room_id).await?;

        Ok(res)
    }

    pub async fn subscribe(
        &self,
        client_id: &str,
        target_id: &str,
        renegotiation_callback: RenegotiationCallback,
        ice_candidate_callback: IceCandidateCallback,
    ) -> Result<SubscribeResponse, WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id).await?;
        let mut room = room.lock().await;

        let params = SubscribeParams {
            participant_id: participant_id.to_string(),
            target_id: (&target_id).to_string(),
            on_candidate: ice_candidate_callback,
            on_negotiation_needed: renegotiation_callback,
        };

        let res = room.subscribe(params).await?;

        Ok(res)
    }

    pub async fn set_subscriber_desc(
        &self,
        client_id: &str,
        target_id: &str,
        sdp: &str,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id).await?;
        let room = room.lock().await;

        room.set_subscriber_remote_sdp(target_id, participant_id, sdp)
            .await?;

        Ok(())
    }

    pub async fn handle_publisher_renegotiation(
        &self,
        client_id: &str,
        sdp: &str,
    ) -> Result<String, WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id).await?;
        let room = room.lock().await;

        let sdp = room
            .handle_publisher_renegotiation(participant_id, sdp)
            .await?;

        Ok(sdp)
    }

    pub async fn add_publisher_candidate(
        &self,
        client_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id).await?;
        let room = room.lock().await;

        room.add_publisher_candidate(participant_id, candidate)
            .await?;

        Ok(())
    }

    pub async fn add_subscriber_candidate(
        &self,
        client_id: &str,
        target_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id).await?;
        let room = room.lock().await;

        room.add_subscriber_candidate(target_id, participant_id, candidate)
            .await?;

        Ok(())
    }

    pub async fn leave_room(self, client_id: &str) -> Result<WClient, WebRTCError> {
        let client = self.get_client_by_id(client_id).await?.clone();
        let room_id = &client.room_id;
        let participant_id = client.participant_id.clone();

        let room = self._get_room_by_id(room_id).await?;

        let mut room_clone_for_leave = {
            let room_guard = room.lock().await;
            room_guard.clone()
        };

        room_clone_for_leave.leave_room(&participant_id).await;

        self.remove_client(client_id).await;

        Ok(client)
    }

    pub async fn set_audio_enabled(
        &self,
        client_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id).await?;
        let room = room.lock().await;

        room.set_audio_enabled(&participant_id, is_enabled).await?;

        Ok(())
    }

    pub async fn set_video_enabled(
        &self,
        client_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id).await?;
        let room = room.lock().await;

        room.set_video_enabled(&participant_id, is_enabled).await?;

        Ok(())
    }

    pub async fn set_camera_type(
        &self,
        client_id: &str,
        camera_type: u8,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id).await?;
        let room = room.lock().await;

        room.set_camera_type(&participant_id, camera_type).await?;

        Ok(())
    }

    pub async fn set_e2ee_enabled(
        &self,
        client_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id).await?;
        let room = room.lock().await;

        room.set_e2ee_enabled(&participant_id, is_enabled).await?;

        Ok(())
    }

    pub async fn set_screen_sharing(
        &self,
        client_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id).await?;
        let room = room.lock().await;

        room.set_screen_sharing(&participant_id, is_enabled).await?;

        Ok(())
    }

    pub async fn set_hand_raising(
        &self,
        client_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let client = self.get_client_by_id(client_id).await?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id).await?;
        let room = room.lock().await;

        room.set_hand_raising(&participant_id, is_enabled).await?;

        Ok(())
    }

    pub async fn add_client(self, client_id: &str, info: WClient) {
        let mut clients = self.clients.lock().await;

        if !clients.contains_key(client_id) {
            clients.insert(client_id.to_string(), info.clone());
        }
    }

    pub async fn remove_client(self, client_id: &str) {
        let mut clients = self.clients.lock().await;

        clients.remove(client_id);
    }

    async fn _add_room(&self, room_id: &str) -> Result<Arc<Mutex<Room>>, WebRTCError> {
        let room_value = Arc::new(Mutex::new(Room::new()));

        let mut rooms = self.rooms.lock().await;
        rooms.insert(room_id.to_string(), room_value.clone());

        Ok(room_value)
    }

    pub async fn get_client_by_id(&self, client_id: &str) -> Result<WClient, WebRTCError> {
        let clients = self.clients.lock().await;
        if let Some(client) = clients.get(client_id) {
            Ok(client.clone())
        } else {
            Err(WebRTCError::ParticipantNotFound)
        }
    }

    pub async fn _get_room_by_id(&self, room_id: &str) -> Result<Arc<Mutex<Room>>, WebRTCError> {
        let rooms = self.rooms.lock().await;
        if let Some(room) = rooms.get(room_id) {
            Ok(room.clone())
        } else {
            Err(WebRTCError::RoomNotFound)
        }
    }
}
