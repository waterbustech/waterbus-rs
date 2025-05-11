use chrono::Utc;
use salvo::async_trait;

use crate::{
    core::{
        entities::models::{Ccu, NewCcu},
        types::{
            errors::{ccu_error::CcuError, room_error::RoomError},
            res::room_response::ParticipantResponse,
        },
    },
    features::{
        ccu::repository::{CcuRepository, CcuRepositoryImpl},
        room::repository::{RoomRepository, RoomRepositoryImpl},
    },
};

#[async_trait]
pub trait SfuService {
    async fn create_ccu(&self, socket_id: &str, user_id: i32) -> Result<Ccu, CcuError>;

    async fn delete_ccu(&self, socket_id: &str) -> Result<(), CcuError>;

    // async fn get_ccu_by_id(&self, ccu_id: i32) -> Result<Ccu, CcuError>;

    async fn update_participant(
        &self,
        participant_id: i32,
        socket_id: &str,
    ) -> Result<ParticipantResponse, RoomError>;

    async fn delete_participant(&self, participant_id: i32) -> Result<(), RoomError>;
}

#[derive(Debug, Clone)]
pub struct SfuServiceImpl {
    ccu_repository: CcuRepositoryImpl,
    room_repository: RoomRepositoryImpl,
}

impl SfuServiceImpl {
    pub fn new(ccu_repository: CcuRepositoryImpl, room_repository: RoomRepositoryImpl) -> Self {
        Self {
            ccu_repository,
            room_repository,
        }
    }
}

#[async_trait]
impl SfuService for SfuServiceImpl {
    async fn create_ccu(&self, socket_id: &str, user_id: i32) -> Result<Ccu, CcuError> {
        let now = Utc::now().naive_utc();

        let new_ccu = NewCcu {
            socket_id: socket_id,
            user_id: Some(user_id),
            pod_name: "pod_name_1",
            created_at: now,
        };

        let ccu = self.ccu_repository.create_ccu(new_ccu).await?;

        Ok(ccu)
    }

    async fn delete_ccu(&self, socket_id: &str) -> Result<(), CcuError> {
        let _ = self.ccu_repository.delete_ccu_by_id(socket_id).await?;

        Ok(())
    }

    // async fn get_ccu_by_id(&self, ccu_id: i32) -> Result<Ccu, CcuError> {
    //     let ccu = self.ccu_repository.get_ccu_by_id(ccu_id).await?;

    //     Ok(ccu)
    // }

    async fn update_participant(
        &self,
        participant_id: i32,
        socket_id: &str,
    ) -> Result<ParticipantResponse, RoomError> {
        let ccu = self
            .ccu_repository
            .get_ccu_by_socket_id(socket_id)
            .await
            .map_err(|_| RoomError::UnexpectedError("CCU not found".into()))?;

        let participant = self
            .room_repository
            .get_participant_by_id(participant_id)
            .await?;

        let mut participant = participant.participant;

        participant.ccu_id = Some(ccu.id);

        let participant = self.room_repository.update_participant(participant).await?;

        Ok(participant)
    }

    async fn delete_participant(&self, participant_id: i32) -> Result<(), RoomError> {
        let _ = self
            .room_repository
            .delete_participant_by_id(participant_id)
            .await?;

        Ok(())
    }
}
