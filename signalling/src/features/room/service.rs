use crate::core::dtos::common::pagination_dto::PaginationDto;
use crate::core::dtos::room::create_room_dto::CreateRoomDto;
use crate::core::dtos::room::update_room_dto::UpdateRoomDto;
use crate::core::entities::models::{
    MembersRoleEnum, NewMember, NewParticipant, NewRoom, ParticipantsStatusEnum, RoomStatusEnum,
};
use crate::core::types::errors::room_error::RoomError;
use crate::core::types::responses::room_response::{ParticipantResponse, RoomResponse};
use crate::core::utils::bcrypt_utils::{hash_password, verify_password};
use crate::core::utils::id_utils::generate_room_code;
use crate::features::room::repository::RoomRepository;
use crate::features::user::repository::UserRepository;
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
pub struct RoomServiceImpl<R: RoomRepository, U: UserRepository> {
    room_repository: R,
    user_repository: U,
}

impl<R: RoomRepository, U: UserRepository> RoomServiceImpl<R, U> {
    pub fn new(room_repository: R, user_repository: U) -> Self {
        Self {
            room_repository,
            user_repository,
        }
    }
}

#[async_trait]
impl<R: RoomRepository + Send + Sync, U: UserRepository + Send + Sync> RoomService
    for RoomServiceImpl<R, U>
{
    async fn create_room(
        &self,
        data: CreateRoomDto,
        user_id: i32,
    ) -> Result<RoomResponse, RoomError> {
        let user = self
            .user_repository
            .get_user_by_id(user_id)
            .await
            .map_err(|_| RoomError::UnexpectedError("User not found".into()))?;

        let (password_hashed, code) = tokio::try_join!(
            {
                let password = data.password.clone();
                async move {
                    match password {
                        Some(pwd) => tokio::task::spawn_blocking(move || Some(hash_password(&pwd)))
                            .await
                            .map_err(|_| {
                                RoomError::UnexpectedError("Failed to hash password".into())
                            }),
                        None => Ok(None),
                    }
                }
            },
            self.generate_unique_room_code(10),
        )?;

        let now = Utc::now().naive_utc();

        let new_room = NewRoom {
            title: &data.title,
            password: password_hashed.as_deref(),
            code: &code,
            status: RoomStatusEnum::Active.into(),
            created_at: now,
            updated_at: now,
            latest_message_created_at: now,
            type_: data.room_type as i16,
            capacity: data.capacity,
            streaming_protocol: Some(data.streaming_protocol as i16),
        };

        self.room_repository
            .create_room_with_member(new_room, user, now)
            .await
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
            member.member.user_id == user_id && member.member.role == MembersRoleEnum::Owner as i16
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

        if let Some(room_type) = update_room_dto.room_type {
            room.type_ = room_type as i16;
        }
        if let Some(streaming_protocol) = update_room_dto.streaming_protocol {
            room.streaming_protocol = Some(streaming_protocol as i16);
        }
        if let Some(capacity) = update_room_dto.capacity {
            room.capacity = Some(capacity);
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
            .position(|member| member.member.user_id == user_id)
            .ok_or_else(|| RoomError::UnexpectedError("Member not found".into()))?;

        let member = room.members[index_of_member].member.clone();

        if member.role == MembersRoleEnum::Owner as i16 {
            return Err(RoomError::UnexpectedError("Host not allowed to leave the room. You can archive chats if the room no longer active.".into()));
        }

        self.room_repository.delete_member_by_id(member.id).await?;

        room.members
            .retain(|member| member.member.user_id != user_id);

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
            .any(|member| member.member.user_id == user_id);

        // Enforce capacity
        if !is_member {
            if let Some(capacity) = room.room.capacity {
                if room.members.len() as i32 >= capacity {
                    return Err(RoomError::RoomIsFull);
                }
            }
        }

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
            status: ParticipantsStatusEnum::Active.into(),
            created_at: now,
        };

        let participant = self.room_repository.create_participant(participant).await?;

        room.participants
            .retain(|p| p.participant.node_id.is_some());
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
            .any(|member| member.member.user_id == user_id);

        if is_member {
            return Err(RoomError::UnexpectedError(
                "User already in the room".to_string(),
            ));
        }

        let is_host = room.members.iter().any(|member| {
            member.member.user_id == host_id && member.member.role == MembersRoleEnum::Owner as i16
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
            role: MembersRoleEnum::Attendee.into(),
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
            .position(|member| member.member.user_id == user_id)
            .ok_or_else(|| RoomError::UnexpectedError("Member not found".into()))?;

        let is_host = room.members.iter().any(|member| {
            member.member.user_id == host_id && member.member.role == MembersRoleEnum::Owner as i16
        });

        if !is_host {
            return Err(RoomError::YouDontHavePermissions);
        }

        let member_id = room.members[index_of_member].member.id;

        self.room_repository.delete_member_by_id(member_id).await?;

        room.members
            .retain(|member| member.member.user_id != user_id);

        Ok(room)
    }

    async fn deactivate_room(&self, room_id: i32, user_id: i32) -> Result<RoomResponse, RoomError> {
        let room = self.room_repository.get_room_by_id(room_id).await?;

        let index_of_member = room
            .members
            .iter()
            .position(|member| member.member.user_id == user_id)
            .ok_or_else(|| RoomError::UnexpectedError("Member not found".into()))?;

        let member = room.members[index_of_member].member.clone();

        if member.role != MembersRoleEnum::Owner as i16 {
            return Err(RoomError::YouDontHavePermissions);
        }

        let mut room = room.room;

        room.status = RoomStatusEnum::Inactive as i16;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dtos::room::create_room_dto::CreateRoomDto;
    use crate::core::dtos::room::update_room_dto::UpdateRoomDto;
    use crate::core::entities::models::{
        Member, Message, Participant, Room, RoomType, StreamingProtocol, User,
    };
    use crate::core::types::responses::message_response::MessageResponse;
    use crate::core::types::responses::room_response::{
        MemberResponse, ParticipantResponse, RoomResponse,
    };
    use chrono::{DateTime, NaiveDateTime};
    use salvo::async_trait;
    use std::sync::{Arc, Mutex};

    // Sample data helpers
    fn sample_user(id: i32) -> User {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        User {
            id,
            full_name: Some(format!("User{id}")),
            user_name: format!("user{id}"),
            bio: Some("bio".to_string()),
            external_id: format!("ext{id}"),
            avatar: Some("avatar.png".to_string()),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            last_seen_at: None,
        }
    }

    fn sample_member(id: i32, user_id: i32, room_id: i32, role: i16) -> Member {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        Member {
            id,
            role,
            created_at: now,
            deleted_at: None,
            soft_deleted_at: None,
            user_id,
            room_id,
        }
    }

    fn sample_participant(
        id: i32,
        user_id: i32,
        room_id: i32,
        node_id: Option<String>,
    ) -> Participant {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        Participant {
            id,
            created_at: now,
            deleted_at: None,
            user_id,
            room_id,
            status: ParticipantsStatusEnum::Active as i16,
            node_id,
        }
    }

    fn sample_message(id: i32, user_id: i32, room_id: i32) -> Message {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        Message {
            id,
            data: "Hello".to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            created_by_id: user_id,
            room_id,
            type_: 0,
            status: 0,
        }
    }

    fn sample_room(id: i32, owner_id: i32) -> RoomResponse {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        RoomResponse {
            room: Room {
                id,
                title: format!("Room{id}"),
                password: None,
                avatar: None,
                status: RoomStatusEnum::Active as i16,
                latest_message_created_at: Some(now),
                code: format!("CODE{id}"),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                latest_message_id: Some(1),
                type_: RoomType::Conferencing as i16,
                capacity: Some(10),
                streaming_protocol: Some(StreamingProtocol::SFU as i16),
            },
            members: vec![MemberResponse {
                member: sample_member(1, owner_id, id, MembersRoleEnum::Owner as i16),
                user: Some(sample_user(owner_id)),
            }],
            participants: vec![ParticipantResponse {
                participant: sample_participant(1, owner_id, id, Some("node1".to_string())),
                user: Some(sample_user(owner_id)),
            }],
            latest_message: Some(MessageResponse {
                message: sample_message(1, owner_id, id),
                created_by: Some(sample_user(owner_id)),
                room: None,
            }),
            is_protected: None,
        }
    }

    fn sample_create_room_dto() -> CreateRoomDto {
        CreateRoomDto {
            title: "Test Room".to_string(),
            password: None,
            room_type: RoomType::Conferencing,
            streaming_protocol: StreamingProtocol::SFU,
            capacity: Some(10),
        }
    }

    fn sample_update_room_dto() -> UpdateRoomDto {
        UpdateRoomDto {
            title: Some("Updated Room".to_string()),
            password: Some("newpass".to_string()),
            avatar: Some("avatar.png".to_string()),
            room_type: Some(RoomType::Conferencing),
            streaming_protocol: Some(StreamingProtocol::SFU),
            capacity: Some(10),
        }
    }

    // Mock RoomRepository
    #[derive(Clone)]
    struct MockRoomRepository {
        pub rooms: Arc<Mutex<Vec<RoomResponse>>>,
        pub fail: bool,
    }

    #[async_trait]
    impl RoomRepository for MockRoomRepository {
        async fn find_all(
            &self,
            _user_id: i32,
            _status: RoomStatusEnum,
            _skip: i64,
            _limit: i64,
        ) -> Result<Vec<RoomResponse>, RoomError> {
            Ok(self.rooms.lock().unwrap().clone())
        }
        async fn exists_code(&self, code: &str) -> Result<bool, RoomError> {
            let rooms = self.rooms.lock().unwrap();
            Ok(rooms.iter().any(|r| r.room.code == code))
        }
        async fn get_room_by_id(&self, room_id: i32) -> Result<RoomResponse, RoomError> {
            let rooms = self.rooms.lock().unwrap();
            rooms
                .iter()
                .find(|r| r.room.id == room_id)
                .cloned()
                .ok_or(RoomError::UnexpectedError("not found".into()))
        }
        async fn get_room_by_code(&self, code: &str) -> Result<RoomResponse, RoomError> {
            let rooms = self.rooms.lock().unwrap();
            rooms
                .iter()
                .find(|r| r.room.code == code)
                .cloned()
                .ok_or(RoomError::UnexpectedError("not found".into()))
        }
        async fn create_room(&self, room: NewRoom<'_>) -> Result<RoomResponse, RoomError> {
            let mut response = sample_room(1, 1);
            response.room.title = room.title.to_string();
            Ok(response)
        }
        async fn create_room_with_member(
            &self,
            room: NewRoom<'_>,
            _user: User,
            _now: NaiveDateTime,
        ) -> Result<RoomResponse, RoomError> {
            if self.fail {
                Err(RoomError::UnexpectedError("fail".into()))
            } else {
                let mut response = sample_room(1, 1);
                response.room.title = room.title.to_string();
                Ok(response)
            }
        }
        async fn update_room(&self, room: Room) -> Result<RoomResponse, RoomError> {
            let mut rooms = self.rooms.lock().unwrap();
            if let Some(r) = rooms.iter_mut().find(|r| r.room.id == room.id) {
                r.room = room;
                Ok(r.clone())
            } else {
                Err(RoomError::UnexpectedError("not found".into()))
            }
        }
        async fn get_member_by_id(&self, member_id: i32) -> Result<MemberResponse, RoomError> {
            let rooms = self.rooms.lock().unwrap();
            for r in rooms.iter() {
                for m in &r.members {
                    if m.member.id == member_id {
                        let member_clone = m.clone();
                        return Ok(MemberResponse {
                            member: member_clone.member,
                            user: Some(sample_user(m.member.user_id)),
                        });
                    }
                }
            }
            Err(RoomError::UnexpectedError("not found".into()))
        }
        async fn create_member(&self, _member: NewMember<'_>) -> Result<MemberResponse, RoomError> {
            Ok(MemberResponse {
                member: sample_member(2, 2, 1, MembersRoleEnum::Attendee as i16),
                user: Some(sample_user(2)),
            })
        }
        async fn update_member(&self, member: Member) -> Result<MemberResponse, RoomError> {
            let member_clone = member.clone();
            Ok(MemberResponse {
                member,
                user: Some(sample_user(member_clone.user_id)),
            })
        }
        async fn delete_member_by_id(&self, _member_id: i32) -> Result<(), RoomError> {
            Ok(())
        }
        async fn get_participant_by_id(&self, _id: i32) -> Result<ParticipantResponse, RoomError> {
            let participant = sample_participant(1, 1, 1, Some("node1".to_string()));
            let participant_clone = participant.clone();
            Ok(ParticipantResponse {
                participant,
                user: Some(sample_user(participant_clone.user_id)),
            })
        }
        async fn create_participant(
            &self,
            _participant: NewParticipant<'_>,
        ) -> Result<ParticipantResponse, RoomError> {
            Ok(ParticipantResponse {
                participant: sample_participant(1, 1, 1, Some("node1".to_string())),
                user: Some(sample_user(1)),
            })
        }
        async fn update_participant(
            &self,
            participant: Participant,
        ) -> Result<ParticipantResponse, RoomError> {
            let participant_clone = participant.clone();
            Ok(ParticipantResponse {
                participant,
                user: Some(sample_user(participant_clone.user_id)),
            })
        }
        async fn delete_participant_by_id(&self, _id: i32) -> Result<(), RoomError> {
            Ok(())
        }
        async fn delete_participants_by_node(&self, _node_id: &str) -> Result<(), RoomError> {
            Ok(())
        }
    }

    // Mock UserRepository
    #[derive(Clone)]
    struct MockUserRepository {
        pub users: Arc<Mutex<Vec<User>>>,
        pub fail: bool,
    }

    #[async_trait]
    impl UserRepository for MockUserRepository {
        async fn get_user_by_id(
            &self,
            id: i32,
        ) -> Result<User, crate::core::types::errors::user_error::UserError> {
            if self.fail {
                Err(crate::core::types::errors::user_error::UserError::UserNotFound(id))
            } else {
                let users = self.users.lock().unwrap();
                users
                    .iter()
                    .find(|u| u.id == id)
                    .cloned()
                    .ok_or(crate::core::types::errors::user_error::UserError::UserNotFound(id))
            }
        }
        async fn update_user(
            &self,
            user: User,
        ) -> Result<User, crate::core::types::errors::user_error::UserError> {
            Ok(user)
        }
        async fn get_username(
            &self,
            username: &str,
        ) -> Result<String, crate::core::types::errors::user_error::UserError> {
            Ok(username.to_string())
        }
        async fn update_username(
            &self,
            user_id: i32,
            username: &str,
        ) -> Result<User, crate::core::types::errors::user_error::UserError> {
            let mut users = self.users.lock().unwrap();
            if let Some(user) = users.iter_mut().find(|u| u.id == user_id) {
                user.user_name = username.to_string();
                Ok(user.clone())
            } else {
                Err(crate::core::types::errors::user_error::UserError::UserNotFound(user_id))
            }
        }
    }

    // Test scaffolding for RoomService methods will be added here

    #[tokio::test]
    async fn test_create_room_success() {
        let rooms = Arc::new(Mutex::new(vec![]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let dto = sample_create_room_dto();
        let result = service.create_room(dto, 1).await;
        assert!(result.is_ok());
        let room = result.unwrap();
        assert_eq!(room.room.title, "Test Room");
        assert_eq!(room.room.type_, RoomType::Conferencing as i16);
        assert_eq!(
            room.room.streaming_protocol,
            Some(StreamingProtocol::SFU as i16)
        );
        assert_eq!(room.room.capacity, Some(10));
    }

    #[tokio::test]
    async fn test_create_room_user_not_found() {
        let rooms = Arc::new(Mutex::new(vec![]));
        let users = Arc::new(Mutex::new(vec![]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let dto = sample_create_room_dto();
        let result = service.create_room(dto, 99).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_room_success() {
        let room = sample_room(1, 1);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let dto = sample_update_room_dto();
        let result = service.update_room(dto, 1, 1).await;
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.room.title, "Updated Room");
        assert_eq!(updated.room.type_, RoomType::Conferencing as i16);
        assert_eq!(
            updated.room.streaming_protocol,
            Some(StreamingProtocol::SFU as i16)
        );
        assert_eq!(updated.room.capacity, Some(10));
    }

    #[tokio::test]
    async fn test_update_room_not_host() {
        let room = sample_room(1, 1);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(2)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let dto = sample_update_room_dto();
        let result = service.update_room(dto, 1, 2).await;
        assert!(matches!(result, Err(RoomError::YouDontHavePermissions)));
    }

    #[tokio::test]
    async fn test_get_rooms_by_status() {
        let room = sample_room(1, 1);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let pagination = PaginationDto { skip: 0, limit: 10 };
        let result = service
            .get_rooms_by_status(RoomStatusEnum::Active as i32, 1, pagination)
            .await;
        assert!(result.is_ok());
        let list = result.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_get_room_by_id_success() {
        let room = sample_room(1, 1);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.get_room_by_id(1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().room.id, 1);
    }

    #[tokio::test]
    async fn test_get_room_by_id_not_found() {
        let rooms = Arc::new(Mutex::new(vec![]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.get_room_by_id(99).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_leave_room_success() {
        let mut room = sample_room(1, 1);
        // Add a non-owner member
        let member = sample_member(2, 2, 1, MembersRoleEnum::Attendee as i16);
        let member_resp = MemberResponse {
            member: member.clone(),
            user: Some(sample_user(2)),
        };
        room.members.push(member_resp);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(2)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.leave_room(1, 2).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_leave_room_owner_error() {
        let room = sample_room(1, 1);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.leave_room(1, 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_join_room_success() {
        let mut room = sample_room(1, 1);
        // Remove user 2 from members to test join
        room.members.retain(|m| m.member.user_id != 2);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(2)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.join_room(2, 1, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_join_room_full() {
        let mut room = sample_room(1, 1);
        // Set capacity to 3 (owner + 2 attendees)
        room.room.capacity = Some(3);
        room.members.push(MemberResponse {
            member: sample_member(2, 2, 1, MembersRoleEnum::Attendee as i16),
            user: Some(sample_user(2)),
        });
        room.members.push(MemberResponse {
            member: sample_member(3, 3, 1, MembersRoleEnum::Attendee as i16),
            user: Some(sample_user(3)),
        });
        // Now the room is full (3/3)
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(4)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.join_room(4, 1, None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), RoomError::RoomIsFull);
    }

    #[tokio::test]
    async fn test_add_member_success() {
        let room = sample_room(1, 1);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(2)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.add_member(1, 1, 2).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_add_member_already_in_room() {
        let room = sample_room(1, 1);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.add_member(1, 1, 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remove_member_success() {
        let mut room = sample_room(1, 1);
        let member = sample_member(2, 2, 1, MembersRoleEnum::Attendee as i16);
        let member_resp = MemberResponse {
            member: member.clone(),
            user: Some(sample_user(2)),
        };
        room.members.push(member_resp);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.remove_member(1, 1, 2).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_remove_member_not_host() {
        let mut room = sample_room(1, 1);
        let member = sample_member(2, 2, 1, MembersRoleEnum::Attendee as i16);
        let member_resp = MemberResponse {
            member: member.clone(),
            user: Some(sample_user(2)),
        };
        room.members.push(member_resp);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(2)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.remove_member(1, 2, 1).await;
        assert!(matches!(result, Err(RoomError::YouDontHavePermissions)));
    }

    #[tokio::test]
    async fn test_deactivate_room_success() {
        let room = sample_room(1, 1);
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.deactivate_room(1, 1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().room.status, RoomStatusEnum::Inactive as i16);
    }

    #[tokio::test]
    async fn test_deactivate_room_not_owner() {
        let mut room = sample_room(1, 1);
        // Add a non-owner member (user_id = 2)
        let member = sample_member(2, 2, 1, MembersRoleEnum::Attendee as i16);
        let member_resp = MemberResponse {
            member: member.clone(),
            user: Some(sample_user(2)),
        };
        room.members.push(member_resp);

        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(2)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.deactivate_room(1, 2).await;
        assert!(matches!(result, Err(RoomError::YouDontHavePermissions)));
    }

    #[tokio::test]
    async fn test_update_participant_success() {
        let rooms = Arc::new(Mutex::new(vec![sample_room(1, 1)]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.update_participant(1, "node1").await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().participant.node_id,
            Some("node1".to_string())
        );
    }

    #[tokio::test]
    async fn test_delete_participant_success() {
        let rooms = Arc::new(Mutex::new(vec![sample_room(1, 1)]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.delete_participant(1).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_participants_by_node_success() {
        let rooms = Arc::new(Mutex::new(vec![sample_room(1, 1)]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.delete_participants_by_node("node1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_unique_room_code_success() {
        let rooms = Arc::new(Mutex::new(vec![]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        let result = service.generate_unique_room_code(5).await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_generate_unique_room_code_fail() {
        // All codes exist
        let mut room = sample_room(1, 1);
        room.room.code = "DUPLICATE".to_string();
        let rooms = Arc::new(Mutex::new(vec![room.clone()]));
        let users = Arc::new(Mutex::new(vec![sample_user(1)]));
        let room_repo = MockRoomRepository {
            rooms: rooms.clone(),
            fail: false,
        };
        let user_repo = MockUserRepository {
            users: users.clone(),
            fail: false,
        };
        let service = RoomServiceImpl::new(room_repo, user_repo);
        // Patch generate_room_code to always return "DUPLICATE" (simulate collision)
        // Here, just check that after max_attempts, it fails
        let result = service.generate_unique_room_code(0).await;
        assert!(result.is_err());
    }
}
