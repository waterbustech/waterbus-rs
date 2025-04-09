use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    errors::WebRTCError,
    models::{
        IceCandidate, IceCandidateCallback, JoinRoomParams, JoinRoomResponse, JoinedCallback,
        RenegotiationCallback, SubscribeParams, SubscribeResponse, WClient,
    },
    room::Room,
};

#[derive(Debug, Clone)]
pub struct WebRTCManager {
    rooms: HashMap<String, Arc<Mutex<Room>>>,
    clients: HashMap<String, WClient>,
}

pub struct JoinRoomReq {
    pub sdp: String,
    pub is_video_enabled: bool,
    pub is_audio_enabled: bool,
    pub is_e2ee_enabled: bool,
    pub callback: JoinedCallback,
}

impl WebRTCManager {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
            clients: HashMap::new(),
        }
    }

    pub async fn join_room(
        &mut self,
        client_id: &str,
        req: JoinRoomReq,
    ) -> Result<JoinRoomResponse, WebRTCError> {
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let mut room = self._get_room_by_id(room_id);

        if let Err(_) = room {
            room = self._add_room(&room_id);
        }

        let mut room = room.unwrap().lock().unwrap();

        let params = JoinRoomParams {
            participant_id: participant_id.to_string(),
            sdp: req.sdp,
            is_audio_enabled: req.is_audio_enabled,
            is_video_enabled: req.is_video_enabled,
            is_e2ee_enabled: req.is_e2ee_enabled,
            callback: req.callback,
        };

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
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let mut room = room.lock().unwrap();

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
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.lock().unwrap();

        room.set_subscriber_remote_sdp(target_id, participant_id, sdp)
            .await?;

        Ok(())
    }

    pub async fn handle_publisher_renegotiation(
        &self,
        client_id: &str,
        sdp: &str,
    ) -> Result<String, WebRTCError> {
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.lock().unwrap();

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
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.lock().unwrap();

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
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = &client.room_id;
        let participant_id = &client.participant_id;

        let room = self._get_room_by_id(room_id)?;
        let room = room.lock().unwrap();

        room.add_subscriber_candidate(target_id, participant_id, candidate)
            .await?;

        Ok(())
    }

    pub async fn leave_room(&self, client_id: &str) -> Result<WClient, WebRTCError> {
        let client = self._get_client_by_id(client_id)?.clone();
        let room_id = &client.room_id;
        let participant_id = client.participant_id.clone();

        let room = self._get_room_by_id(room_id)?;

        let mut room_clone_for_leave = {
            let room_guard = room.lock().unwrap();
            room_guard.clone()
        };

        room_clone_for_leave.leave_room(&participant_id).await;

        Ok(client)
    }

    pub fn set_audio_enabled(&self, client_id: &str, is_enabled: bool) -> Result<(), WebRTCError> {
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.lock().unwrap();

        room.set_audio_enabled(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_video_enabled(&self, client_id: &str, is_enabled: bool) -> Result<(), WebRTCError> {
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.lock().unwrap();

        room.set_video_enabled(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_camera_type(&self, client_id: &str, camera_type: u8) -> Result<(), WebRTCError> {
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.lock().unwrap();

        room.set_camera_type(&participant_id, camera_type)?;

        Ok(())
    }

    pub fn set_e2ee_enabled(&self, client_id: &str, is_enabled: bool) -> Result<(), WebRTCError> {
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.lock().unwrap();

        room.set_e2ee_enabled(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_screen_sharing(&self, client_id: &str, is_enabled: bool) -> Result<(), WebRTCError> {
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.lock().unwrap();

        room.set_screen_sharing(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn set_hand_raising(&self, client_id: &str, is_enabled: bool) -> Result<(), WebRTCError> {
        let client = self._get_client_by_id(client_id)?;

        let client = client.clone();

        let room_id = client.room_id;
        let participant_id = client.participant_id;

        let room = self._get_room_by_id(&room_id)?;
        let room = room.lock().unwrap();

        room.set_hand_raising(&participant_id, is_enabled)?;

        Ok(())
    }

    pub fn add_client(&mut self, client_id: &str, info: WClient) {
        if let Err(_) = self._get_client_by_id(client_id) {
            self.clients.insert(client_id.to_string(), info.clone());
        }
    }

    fn _add_room(&mut self, room_id: &str) -> Result<&Arc<Mutex<Room>>, WebRTCError> {
        let room_value = Arc::new(Mutex::new(Room::new()));
        self.rooms.insert(room_id.to_string(), room_value);

        return self._get_room_by_id(room_id);
    }

    fn _get_client_by_id(&self, client_id: &str) -> Result<&WClient, WebRTCError> {
        if let Some(client) = self.clients.get(client_id) {
            return Ok(client);
        } else {
            Err(WebRTCError::ParticipantNotFound)
        }
    }

    fn _get_room_by_id(&self, room_id: &str) -> Result<&Arc<Mutex<Room>>, WebRTCError> {
        if let Some(room) = self.rooms.get(room_id) {
            return Ok(room);
        } else {
            return Err(WebRTCError::RoomNotFound);
        }
    }
}
