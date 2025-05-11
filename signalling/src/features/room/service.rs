use crate::core::dtos::common::pagination_dto::PaginationDto;
use crate::core::dtos::room::create_room_dto::CreateRoomDto;
use crate::core::dtos::room::update_room_dto::UpdateRoomDto;
use crate::core::entities::models::{
    MembersRoleEnum, NewMember, NewParticipant, NewRoom, ParticipantsStatusEnum, RoomStatusEnum,
    RoomType,
};
use crate::core::types::errors::room_error::RoomError;
use crate::core::types::res::room_response::{ParticipantResponse, RoomResponse};
use crate::core::utils::bcrypt_utils::{hash_password, verify_password};
use crate::core::utils::id_utils::generate_room_code;
use crate::features::room::repository::{RoomRepository, RoomRepositoryImpl};
use crate::features::user::repository::{UserRepository, UserRepositoryImpl};
use chrono::Utc;
use salvo::async_trait;

#[async_trait]
pub trait RoomService {
    async fn create_room(
        &self,
        data: CreateRoomDto,
        user_id: i32,
    ) -> Result<RoomResponse, RoomError>;

    async fn update_room(
        &self,
        data: UpdateRoomDto,
        room_id: i32,
        user_id: i32,
    ) -> Result<RoomResponse, RoomError>;

    async fn get_rooms_by_status(
        &self,
        room_status: i32,
        user_id: i32,
        pagination_dto: PaginationDto,
    ) -> Result<Vec<RoomResponse>, RoomError>;

    async fn get_room_by_id(&self, room_id: i32) -> Result<RoomResponse, RoomError>;

    async fn get_room_by_code(&self, room_code: &str) -> Result<RoomResponse, RoomError>;

    async fn leave_room(&self, room_id: i32, user_id: i32) -> Result<RoomResponse, RoomError>;

    async fn join_room(
        &self,
        user_id: i32,
        room_id: i32,
        password: Option<&str>,
    ) -> Result<RoomResponse, RoomError>;

    async fn add_member(
        &self,
        room_id: i32,
        host_id: i32,
        user_id: i32,
    ) -> Result<RoomResponse, RoomError>;

    async fn remove_member(
        &self,
        room_id: i32,
        host_id: i32,
        user_id: i32,
    ) -> Result<RoomResponse, RoomError>;

    async fn deactivate_room(&self, room_id: i32, user_id: i32) -> Result<RoomResponse, RoomError>;

    async fn update_participant(
        &self,
        participant_id: i32,
        node_id: &str,
    ) -> Result<ParticipantResponse, RoomError>;

    async fn delete_participant(&self, participant_id: i32) -> Result<(), RoomError>;

    async fn delete_participants_by_node(&self, node_id: &str) -> Result<(), RoomError>;

    async fn generate_unique_room_code(&self, max_attempts: usize) -> Result<String, RoomError>;
}

#[derive(Debug, Clone)]
pub struct RoomServiceImpl {
    room_repository: RoomRepositoryImpl,
    user_repository: UserRepositoryImpl,
}

impl RoomServiceImpl {
    pub fn new(room_repository: RoomRepositoryImpl, user_repository: UserRepositoryImpl) -> Self {
        Self {
            room_repository,
            user_repository,
        }
    }
}

#[async_trait]
impl RoomService for RoomServiceImpl {
    async fn create_room(
        &self,
        data: CreateRoomDto,
        user_id: i32,
    ) -> Result<RoomResponse, RoomError> {
        let now = Utc::now().naive_utc();
        let password_hashed = hash_password(&data.password);

        let code = self.generate_unique_room_code(10).await?;

        let new_room = NewRoom {
            title: &data.title,
            password: &password_hashed,
            code: &code,
            status: RoomStatusEnum::Active as i32,
            created_at: now,
            updated_at: now,
            latest_message_created_at: now,
            type_: RoomType::Conferencing as i32,
        };

        let mut new_room = self.room_repository.create_room(new_room).await?;

        let new_member = NewMember {
            room_id: &new_room.room.id,
            user_id: Some(user_id),
            role: MembersRoleEnum::Host as i32,
            created_at: now,
        };

        let new_member = self.room_repository.create_member(new_member).await?;

        new_room.members = vec![new_member];

        Ok(new_room)
    }

    async fn update_room(
        &self,
        data: UpdateRoomDto,
        room_id: i32,
        user_id: i32,
    ) -> Result<RoomResponse, RoomError> {
        let update_room_dto = data.clone();
        let room = self.room_repository.get_room_by_id(room_id).await?;

        // Check whether user_id is host or not
        let is_host = room.members.iter().any(|member| {
            member.member.user_id == Some(user_id)
                && member.member.role == MembersRoleEnum::Host as i32
        });

        if !is_host {
            return Err(RoomError::YouDontHavePermissions);
        }

        // Update new room metadata
        let mut room = room.room;

        if let Some(title) = update_room_dto.title {
            room.title = title;
        }

        if let Some(password) = update_room_dto.password {
            let password_hashed = hash_password(&password);
            room.password = Some(password_hashed);
        }

        if let Some(avatar) = update_room_dto.avatar {
            room.avatar = Some(avatar);
        }

        let updated_room = self.room_repository.update_room(room).await?;

        Ok(updated_room)
    }

    async fn get_rooms_by_status(
        &self,
        room_status: i32,
        user_id: i32,
        pagination_dto: PaginationDto,
    ) -> Result<Vec<RoomResponse>, RoomError> {
        let room_status = RoomStatusEnum::try_from(room_status).unwrap_or(RoomStatusEnum::Active);

        let pagination_dto = pagination_dto.clone();
        let rooms = self
            .room_repository
            .find_all(
                user_id,
                room_status,
                pagination_dto.skip,
                pagination_dto.limit,
            )
            .await?;

        Ok(rooms)
    }

    async fn get_room_by_id(&self, room_id: i32) -> Result<RoomResponse, RoomError> {
        let room = self.room_repository.get_room_by_id(room_id).await?;

        Ok(room)
    }

    async fn get_room_by_code(&self, room_code: &str) -> Result<RoomResponse, RoomError> {
        let room = self.room_repository.get_room_by_code(room_code).await?;

        Ok(room)
    }

    async fn leave_room(&self, room_id: i32, user_id: i32) -> Result<RoomResponse, RoomError> {
        let mut room = self.room_repository.get_room_by_id(room_id).await?;

        let index_of_member = room
            .members
            .iter()
            .position(|member| member.member.user_id == Some(user_id))
            .ok_or_else(|| RoomError::UnexpectedError("Member not found".into()))?;

        let member = room.members[index_of_member].member.clone();

        if member.role == MembersRoleEnum::Host as i32 {
            return Err(RoomError::UnexpectedError("Host not allowed to leave the room. You can archive chats if the room no longer active.".into()));
        }

        self.room_repository.delete_member_by_id(member.id).await?;

        room.members
            .retain(|member| member.member.user_id != Some(user_id));

        Ok(room)
    }

    async fn join_room(
        &self,
        user_id: i32,
        room_id: i32,
        password: Option<&str>,
    ) -> Result<RoomResponse, RoomError> {
        let _ = self
            .user_repository
            .get_user_by_id(user_id)
            .await
            .map_err(|_| RoomError::UnexpectedError("User not found".into()))?;

        let mut room = self.room_repository.get_room_by_id(room_id).await?;

        let is_member = room
            .members
            .iter()
            .any(|member| member.member.user_id == Some(user_id));

        if !is_member {
            let is_password_correct = match room.room.password.as_ref() {
                Some(hash_password) => match password {
                    Some(pw) => verify_password(pw, hash_password),
                    None => false,
                },
                None => true,
            };

            if !is_password_correct {
                return Err(RoomError::PasswordIncorrect);
            }
        }

        let now = Utc::now().naive_utc();
        let participant = NewParticipant {
            user_id: Some(user_id),
            room_id: &room.room.id,
            status: ParticipantsStatusEnum::Active as i32,
            created_at: now,
        };

        let participant = self.room_repository.create_participant(participant).await?;

        room.participants.retain(|p| p.participant.node_id != None);
        room.participants.push(participant);

        Ok(room)
    }

    async fn add_member(
        &self,
        room_id: i32,
        host_id: i32,
        user_id: i32,
    ) -> Result<RoomResponse, RoomError> {
        let mut room = self.room_repository.get_room_by_id(room_id).await?;

        let is_member = room
            .members
            .iter()
            .any(|member| member.member.user_id == Some(user_id));

        if is_member {
            return Err(RoomError::UnexpectedError(
                "User already in the room".to_string(),
            ));
        }

        let is_host = room.members.iter().any(|member| {
            member.member.user_id == Some(host_id)
                && member.member.role == MembersRoleEnum::Host as i32
        });

        if !is_host {
            return Err(RoomError::YouDontHavePermissions);
        }

        let _ = self
            .user_repository
            .get_user_by_id(user_id)
            .await
            .map_err(|_| RoomError::UnexpectedError("User not found".to_string()));

        let now = Utc::now().naive_utc();

        let new_member = NewMember {
            user_id: Some(user_id),
            room_id: &room.room.id,
            created_at: now,
            role: MembersRoleEnum::Attendee as i32,
        };

        let new_member = self.room_repository.create_member(new_member).await?;

        room.members.push(new_member);

        Ok(room)
    }

    async fn remove_member(
        &self,
        room_id: i32,
        host_id: i32,
        user_id: i32,
    ) -> Result<RoomResponse, RoomError> {
        let mut room = self.room_repository.get_room_by_id(room_id).await?;

        let index_of_member = room
            .members
            .iter()
            .position(|member| member.member.user_id == Some(user_id))
            .ok_or_else(|| RoomError::UnexpectedError("Member not found".into()))?;

        let is_host = room.members.iter().any(|member| {
            member.member.user_id == Some(host_id)
                && member.member.role == MembersRoleEnum::Host as i32
        });

        if !is_host {
            return Err(RoomError::YouDontHavePermissions);
        }

        let member_id = room.members[index_of_member].member.id;

        self.room_repository.delete_member_by_id(member_id).await?;

        room.members
            .retain(|member| member.member.user_id != Some(user_id));

        Ok(room)
    }

    async fn deactivate_room(&self, room_id: i32, user_id: i32) -> Result<RoomResponse, RoomError> {
        let room = self.room_repository.get_room_by_id(room_id).await?;

        let index_of_member = room
            .members
            .iter()
            .position(|member| member.member.user_id == Some(user_id))
            .ok_or_else(|| RoomError::UnexpectedError("Member not found".into()))?;

        let member = room.members[index_of_member].member.clone();

        if member.role != MembersRoleEnum::Host as i32 {
            return Err(RoomError::YouDontHavePermissions);
        }

        let mut room = room.room;

        room.status = RoomStatusEnum::Inactive as i32;

        let room = self.room_repository.update_room(room).await?;

        Ok(room)
    }

    async fn update_participant(
        &self,
        participant_id: i32,
        node_id: &str,
    ) -> Result<ParticipantResponse, RoomError> {
        let participant = self
            .room_repository
            .get_participant_by_id(participant_id)
            .await?;

        let mut participant = participant.participant;

        participant.node_id = Some(node_id.to_string());

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

    async fn delete_participants_by_node(&self, node_id: &str) -> Result<(), RoomError> {
        let _ = self
            .room_repository
            .delete_participants_by_node(node_id)
            .await?;

        Ok(())
    }

    async fn generate_unique_room_code(&self, max_attempts: usize) -> Result<String, RoomError> {
        for _ in 0..max_attempts {
            let code = generate_room_code();
            let exists = self
                .room_repository
                .exists_code(&code)
                .await
                .map_err(|_| RoomError::UnexpectedError("Failed to check room code".into()))?;

            if !exists {
                return Ok(code);
            }
        }

        Err(RoomError::UnexpectedError(
            "Failed to generate unique room code".into(),
        ))
    }
}
