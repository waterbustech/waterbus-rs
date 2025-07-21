use chrono::{NaiveDateTime, Utc};
use salvo::async_trait;

use crate::{
    core::{
        entities::models::{MessagesStatusEnum, MessagesTypeEnum, NewMessage, Room},
        types::{errors::chat_error::ChatError, responses::message_response::MessageResponse},
    },
    features::{room::repository::RoomRepository, user::repository::UserRepository},
};

use super::repository::ChatRepository;

#[async_trait]
pub trait ChatService: Send + Sync {
    async fn get_messages_by_room(
        &self,
        room_id: i32,
        user_id: i32,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError>;

    async fn create_message(
        &self,
        room_id: i32,
        user_id: i32,
        data: &str,
    ) -> Result<MessageResponse, ChatError>;

    async fn update_message(
        &self,
        message_id: i32,
        user_id: i32,
        data: &str,
    ) -> Result<MessageResponse, ChatError>;

    async fn delete_message_by_id(
        &self,
        message_id: i32,
        user_id: i32,
    ) -> Result<MessageResponse, ChatError>;

    async fn delete_conversation(
        &self,
        conversation_id: i32,
        user_id: i32,
    ) -> Result<Room, ChatError>;

    async fn update_latest_message_created_at(
        &self,
        room: Room,
        now: NaiveDateTime,
        latest_mesage_id: Option<i32>,
    );
}

#[derive(Debug, Clone)]
pub struct ChatServiceImpl<C: ChatRepository, R: RoomRepository, U: UserRepository> {
    chat_repository: C,
    room_repository: R,
    user_repository: U,
}

impl<C: ChatRepository, R: RoomRepository, U: UserRepository> ChatServiceImpl<C, R, U> {
    pub fn new(chat_repository: C, room_repository: R, user_repository: U) -> Self {
        Self {
            chat_repository,
            room_repository,
            user_repository,
        }
    }
}

#[async_trait]
impl<
    C: ChatRepository + Send + Sync,
    R: RoomRepository + Send + Sync,
    U: UserRepository + Send + Sync,
> ChatService for ChatServiceImpl<C, R, U>
{
    async fn get_messages_by_room(
        &self,
        room_id: i32,
        user_id: i32,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError> {
        let room = self
            .room_repository
            .get_room_by_id(room_id)
            .await
            .map_err(|_| ChatError::ConversationNotFound(room_id))?;

        // let is_member = room
        //     .members
        //     .iter()
        //     .any(|member| member.member.user_id == Some(user_id));

        // if !is_member {
        //     return Err(ChatError::Forbidden(
        //         "You not allowed get messages from room that you not stay in there".to_string(),
        //     ));
        // }

        let index_of_user = room
            .members
            .iter()
            .position(|member| member.member.user_id == user_id);

        let deleted_at = match index_of_user {
            Some(index) => {
                let member = &room.members[index].member;
                member.soft_deleted_at.unwrap_or(room.room.created_at)
            }
            None => room.room.created_at,
        };

        let messages = self
            .chat_repository
            .get_messages_by_room(room_id, deleted_at, skip, limit)
            .await?;

        Ok(messages)
    }

    async fn create_message(
        &self,
        room_id: i32,
        user_id: i32,
        data: &str,
    ) -> Result<MessageResponse, ChatError> {
        let user = self
            .user_repository
            .get_user_by_id(user_id)
            .await
            .map_err(|_| ChatError::MemberNotFound(user_id))?;

        let room = self
            .room_repository
            .get_room_by_id(room_id)
            .await
            .map_err(|_| ChatError::ConversationNotFound(room_id))?;

        let now = Utc::now().naive_utc();

        let new_message = NewMessage {
            data,
            created_by_id: Some(&user_id),
            room_id: Some(&room_id),
            status: &MessagesStatusEnum::Active.into(),
            type_: &MessagesTypeEnum::Default.into(),
            created_at: now,
            updated_at: now,
        };

        let new_message = self.chat_repository.create_message(new_message).await?;

        self.update_latest_message_created_at(room.room.clone(), now, Some(new_message.id))
            .await;

        Ok(MessageResponse {
            message: new_message,
            created_by: Some(user),
            room: Some(room.room.clone()),
        })
    }

    async fn update_message(
        &self,
        message_id: i32,
        user_id: i32,
        data: &str,
    ) -> Result<MessageResponse, ChatError> {
        let mut message_response = self.chat_repository.get_message_by_id(message_id).await?;
        let room = message_response.clone().room.unwrap();

        if message_response.message.status == MessagesStatusEnum::Inactive as i16 {
            return Err(ChatError::UnexpectedError(
                "Message has been deleted".to_string(),
            ));
        }

        if message_response.message.created_by_id != user_id {
            return Err(ChatError::Forbidden(
                "You not allowed modify message of other users".to_string(),
            ));
        }

        let now = Utc::now().naive_utc();
        let mut message = message_response.message;

        self.update_latest_message_created_at(room, now, None).await;

        message.data = data.to_string();
        message.updated_at = now;

        let message = self.chat_repository.update_message(message).await?;

        message_response.message = message;

        Ok(message_response)
    }

    async fn delete_message_by_id(
        &self,
        message_id: i32,
        user_id: i32,
    ) -> Result<MessageResponse, ChatError> {
        let mut message_response = self.chat_repository.get_message_by_id(message_id).await?;

        if message_response.message.status == MessagesStatusEnum::Inactive as i16 {
            return Err(ChatError::UnexpectedError(
                "Message has been deleted".to_string(),
            ));
        }

        if message_response.message.created_by_id != user_id {
            return Err(ChatError::Forbidden(
                "You not allowed modify message of other users".to_string(),
            ));
        }

        let mut message = message_response.message;

        message.status = MessagesStatusEnum::Inactive.into();

        let message = self.chat_repository.update_message(message).await?;

        message_response.message = message;

        Ok(message_response)
    }

    async fn delete_conversation(
        &self,
        conversation_id: i32,
        user_id: i32,
    ) -> Result<Room, ChatError> {
        let room = self
            .room_repository
            .get_room_by_id(conversation_id)
            .await
            .map_err(|_| ChatError::ConversationNotFound(conversation_id))?;

        let index_of_member = room
            .members
            .iter()
            .position(|member| member.member.user_id == user_id);

        match index_of_member {
            Some(index) => {
                let now = Utc::now().naive_utc();
                let mut member = room.members[index].member.clone();

                member.soft_deleted_at = Some(now);

                let _ = self
                    .room_repository
                    .update_member(member)
                    .await
                    .map_err(|_| ChatError::UnexpectedError("".to_string()))?;

                Ok(room.room)
            }
            None => Err(ChatError::ConversationNotFound(user_id)),
        }
    }

    async fn update_latest_message_created_at(
        &self,
        room: Room,
        now: NaiveDateTime,
        latest_mesage_id: Option<i32>,
    ) {
        let mut room = room.clone();

        room.latest_message_created_at = Some(now);

        if let Some(id) = latest_mesage_id {
            room.latest_message_id = Some(id);
        }

        let _ = self.room_repository.update_room(room).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entities::models::*;
    use crate::core::types::errors::chat_error::ChatError;
    use crate::core::types::errors::room_error::RoomError;
    use crate::core::types::errors::user_error::UserError;
    use crate::core::types::responses::message_response::MessageResponse;
    use crate::core::types::responses::room_response::{
        MemberResponse, ParticipantResponse, RoomResponse,
    };
    use chrono::DateTime;

    // --- Sample Data Helpers ---
    fn sample_user() -> User {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        User {
            id: 1,
            full_name: Some("Test User".to_string()),
            user_name: "testuser".to_string(),
            bio: Some("bio".to_string()),
            external_id: "extid".to_string(),
            avatar: Some("avatar.png".to_string()),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            last_seen_at: None,
        }
    }

    fn sample_room() -> Room {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        Room {
            id: 1,
            title: "Test Room".to_string(),
            password: None,
            avatar: None,
            status: 0,
            latest_message_created_at: Some(now),
            code: "roomcode".to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            latest_message_id: None,
            type_: 0,
        }
    }

    fn sample_member(user_id: i32, room_id: i32) -> Member {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        Member {
            id: 1,
            role: 0,
            created_at: now,
            deleted_at: None,
            soft_deleted_at: None,
            user_id,
            room_id,
        }
    }

    fn sample_message(user_id: i32, room_id: i32) -> Message {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        Message {
            id: 1,
            data: "Hello".to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            created_by_id: user_id,
            room_id,
            type_: MessagesTypeEnum::Default as i16,
            status: MessagesStatusEnum::Active as i16,
        }
    }

    fn sample_message_response(user_id: i32, room_id: i32) -> MessageResponse {
        MessageResponse {
            message: sample_message(user_id, room_id),
            created_by: Some(sample_user()),
            room: Some(sample_room()),
        }
    }

    fn sample_room_response(user_id: i32, room_id: i32) -> RoomResponse {
        RoomResponse {
            room: sample_room(),
            members: vec![MemberResponse {
                member: sample_member(user_id, room_id),
                user: Some(sample_user()),
            }],
            participants: vec![],
            latest_message: None,
        }
    }

    // --- Mock Repositories ---
    #[derive(Clone)]
    struct MockChatRepository {
        pub messages: Option<Vec<MessageResponse>>,
        pub message: Option<MessageResponse>,
        pub new_message: Option<Message>,
        pub updated_message: Option<Message>,
        pub delete_message: Option<Message>,
        pub fail: Option<ChatError>,
    }

    #[async_trait]
    impl ChatRepository for MockChatRepository {
        async fn get_messages_by_room(
            &self,
            _room_id: i32,
            _deleted_at: NaiveDateTime,
            _skip: i64,
            _limit: i64,
        ) -> Result<Vec<MessageResponse>, ChatError> {
            if let Some(ref err) = self.fail {
                return Err(err.clone());
            }
            Ok(self.messages.clone().unwrap_or_default())
        }
        async fn get_message_by_id(&self, _message_id: i32) -> Result<MessageResponse, ChatError> {
            if let Some(ref err) = self.fail {
                return Err(err.clone());
            }
            self.message
                .clone()
                .ok_or(ChatError::MessageNotFound(_message_id))
        }
        async fn create_message(&self, _message: NewMessage<'_>) -> Result<Message, ChatError> {
            if let Some(ref err) = self.fail {
                return Err(err.clone());
            }
            self.new_message
                .clone()
                .ok_or(ChatError::UnexpectedError("fail create".to_string()))
        }
        async fn update_message(&self, _message: Message) -> Result<Message, ChatError> {
            if let Some(ref err) = self.fail {
                return Err(err.clone());
            }
            self.updated_message
                .clone()
                .ok_or(ChatError::UnexpectedError("fail update".to_string()))
        }
        async fn delete_message_by_id(&self, _message_id: i32) -> Result<Message, ChatError> {
            if let Some(ref err) = self.fail {
                return Err(err.clone());
            }
            self.delete_message
                .clone()
                .ok_or(ChatError::UnexpectedError("fail delete".to_string()))
        }
    }

    #[derive(Clone)]
    struct MockRoomRepository {
        pub room: Option<RoomResponse>,
        pub updated_member: Option<MemberResponse>,
        pub updated_room: Option<RoomResponse>,
        pub fail: Option<ChatError>,
    }

    #[async_trait]
    impl RoomRepository for MockRoomRepository {
        async fn find_all(
            &self,
            _user_id: i32,
            _room_status: RoomStatusEnum,
            _skip: i64,
            _limit: i64,
        ) -> Result<Vec<RoomResponse>, RoomError> {
            unimplemented!()
        }
        async fn exists_code(&self, _room_code: &str) -> Result<bool, RoomError> {
            unimplemented!()
        }
        async fn get_room_by_id(&self, _room_id: i32) -> Result<RoomResponse, RoomError> {
            if let Some(ref err) = self.fail {
                return Err(RoomError::UnexpectedError(format!("{:?}", err)));
            }
            self.room.clone().ok_or(RoomError::RoomNotFound(_room_id))
        }
        async fn get_room_by_code(&self, _room_code: &str) -> Result<RoomResponse, RoomError> {
            unimplemented!()
        }
        async fn create_room(&self, _room: NewRoom<'_>) -> Result<RoomResponse, RoomError> {
            unimplemented!()
        }
        async fn create_room_with_member(
            &self,
            _room: NewRoom<'_>,
            _user: User,
            _created_at: NaiveDateTime,
        ) -> Result<RoomResponse, RoomError> {
            unimplemented!()
        }
        async fn update_room(&self, _room: Room) -> Result<RoomResponse, RoomError> {
            if let Some(ref err) = self.fail {
                return Err(RoomError::UnexpectedError(format!("{:?}", err)));
            }
            self.updated_room
                .clone()
                .ok_or(RoomError::UnexpectedError("fail update room".to_string()))
        }
        async fn get_member_by_id(&self, _member_id: i32) -> Result<MemberResponse, RoomError> {
            unimplemented!()
        }
        async fn create_member(&self, _member: NewMember<'_>) -> Result<MemberResponse, RoomError> {
            unimplemented!()
        }
        async fn update_member(&self, _member: Member) -> Result<MemberResponse, RoomError> {
            if let Some(ref err) = self.fail {
                return Err(RoomError::UnexpectedError(format!("{:?}", err)));
            }
            self.updated_member
                .clone()
                .ok_or(RoomError::UnexpectedError("fail update member".to_string()))
        }
        async fn delete_member_by_id(&self, _member_id: i32) -> Result<(), RoomError> {
            unimplemented!()
        }
        async fn get_participant_by_id(
            &self,
            _participant_id: i32,
        ) -> Result<ParticipantResponse, RoomError> {
            unimplemented!()
        }
        async fn create_participant(
            &self,
            _participant: NewParticipant<'_>,
        ) -> Result<ParticipantResponse, RoomError> {
            unimplemented!()
        }
        async fn update_participant(
            &self,
            _participant: Participant,
        ) -> Result<ParticipantResponse, RoomError> {
            unimplemented!()
        }
        async fn delete_participant_by_id(&self, _participant_id: i32) -> Result<(), RoomError> {
            unimplemented!()
        }
        async fn delete_participants_by_node(&self, _node_id: &str) -> Result<(), RoomError> {
            unimplemented!()
        }
    }

    #[derive(Clone)]
    struct MockUserRepository {
        pub user: Option<User>,
        pub fail: Option<ChatError>,
    }

    #[async_trait]
    impl UserRepository for MockUserRepository {
        async fn get_user_by_id(&self, _user_id: i32) -> Result<User, UserError> {
            if let Some(ref err) = self.fail {
                return Err(UserError::UnexpectedError(format!("{:?}", err)));
            }
            self.user.clone().ok_or(UserError::UserNotFound(_user_id))
        }
        async fn update_user(&self, _user: User) -> Result<User, UserError> {
            unimplemented!()
        }
        async fn get_username(&self, _username: &str) -> Result<String, UserError> {
            unimplemented!()
        }
        async fn update_username(&self, _user_id: i32, _username: &str) -> Result<User, UserError> {
            unimplemented!()
        }
    }

    // --- Tests ---
    #[tokio::test]
    async fn test_get_messages_by_room_success() {
        let chat_repo = MockChatRepository {
            messages: Some(vec![sample_message_response(1, 1)]),
            message: None,
            new_message: None,
            updated_message: None,
            delete_message: None,
            fail: None,
        };
        let room_repo = MockRoomRepository {
            room: Some(sample_room_response(1, 1)),
            updated_member: None,
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: Some(sample_user()),
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.get_messages_by_room(1, 1, 0, 10).await;
        assert!(result.is_ok());
        let messages = result.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message.data, "Hello");
    }

    #[tokio::test]
    async fn test_get_messages_by_room_conversation_not_found() {
        let chat_repo = MockChatRepository {
            messages: None,
            message: None,
            new_message: None,
            updated_message: None,
            delete_message: None,
            fail: None,
        };
        let room_repo = MockRoomRepository {
            room: None,
            updated_member: None,
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: Some(sample_user()),
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.get_messages_by_room(1, 1, 0, 10).await;
        assert!(matches!(result, Err(ChatError::ConversationNotFound(1))));
    }

    #[tokio::test]
    async fn test_create_message_success() {
        let chat_repo = MockChatRepository {
            messages: None,
            message: None,
            new_message: Some(sample_message(1, 1)),
            updated_message: None,
            delete_message: None,
            fail: None,
        };
        let room_repo = MockRoomRepository {
            room: Some(sample_room_response(1, 1)),
            updated_member: None,
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: Some(sample_user()),
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.create_message(1, 1, "Hello").await;
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.message.data, "Hello");
    }

    #[tokio::test]
    async fn test_create_message_user_not_found() {
        let chat_repo = MockChatRepository {
            messages: None,
            message: None,
            new_message: None,
            updated_message: None,
            delete_message: None,
            fail: None,
        };
        let room_repo = MockRoomRepository {
            room: Some(sample_room_response(1, 1)),
            updated_member: None,
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: None,
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.create_message(1, 1, "Hello").await;
        assert!(matches!(result, Err(ChatError::MemberNotFound(1))));
    }

    #[tokio::test]
    async fn test_update_message_success() {
        let chat_repo = MockChatRepository {
            messages: None,
            message: Some(sample_message_response(1, 1)),
            new_message: None,
            updated_message: Some(sample_message(1, 1)),
            delete_message: None,
            fail: None,
        };
        let room_repo = MockRoomRepository {
            room: Some(sample_room_response(1, 1)),
            updated_member: None,
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: Some(sample_user()),
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.update_message(1, 1, "Updated").await;
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.message.data, "Hello"); // sample_message always returns "Hello"
    }

    #[tokio::test]
    async fn test_update_message_forbidden() {
        let mut msg = sample_message_response(2, 1); // created_by_id != user_id
        msg.message.created_by_id = 2;
        let chat_repo = MockChatRepository {
            messages: None,
            message: Some(msg),
            new_message: None,
            updated_message: Some(sample_message(2, 1)),
            delete_message: None,
            fail: None,
        };
        let room_repo = MockRoomRepository {
            room: Some(sample_room_response(1, 1)),
            updated_member: None,
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: Some(sample_user()),
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.update_message(1, 1, "Updated").await;
        assert!(matches!(result, Err(ChatError::Forbidden(_))));
    }

    #[tokio::test]
    async fn test_delete_message_by_id_success() {
        let chat_repo = MockChatRepository {
            messages: None,
            message: Some(sample_message_response(1, 1)),
            new_message: None,
            updated_message: Some(sample_message(1, 1)),
            delete_message: Some(sample_message(1, 1)),
            fail: None,
        };
        let room_repo = MockRoomRepository {
            room: Some(sample_room_response(1, 1)),
            updated_member: None,
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: Some(sample_user()),
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.delete_message_by_id(1, 1).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_message_by_id_forbidden() {
        let mut msg = sample_message_response(2, 1); // created_by_id != user_id
        msg.message.created_by_id = 2;
        let chat_repo = MockChatRepository {
            messages: None,
            message: Some(msg),
            new_message: None,
            updated_message: Some(sample_message(2, 1)),
            delete_message: Some(sample_message(2, 1)),
            fail: None,
        };
        let room_repo = MockRoomRepository {
            room: Some(sample_room_response(1, 1)),
            updated_member: None,
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: Some(sample_user()),
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.delete_message_by_id(1, 1).await;
        assert!(matches!(result, Err(ChatError::Forbidden(_))));
    }

    #[tokio::test]
    async fn test_delete_conversation_success() {
        let chat_repo = MockChatRepository {
            messages: None,
            message: None,
            new_message: None,
            updated_message: None,
            delete_message: None,
            fail: None,
        };
        let mut room_resp = sample_room_response(1, 1);
        // Add a member with user_id = 1
        room_resp.members[0].member.user_id = 1;
        let room_repo = MockRoomRepository {
            room: Some(room_resp),
            updated_member: Some(MemberResponse {
                member: sample_member(1, 1),
                user: Some(sample_user()),
            }),
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: Some(sample_user()),
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.delete_conversation(1, 1).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_conversation_not_found() {
        let chat_repo = MockChatRepository {
            messages: None,
            message: None,
            new_message: None,
            updated_message: None,
            delete_message: None,
            fail: None,
        };
        let room_repo = MockRoomRepository {
            room: None,
            updated_member: None,
            updated_room: None,
            fail: None,
        };
        let user_repo = MockUserRepository {
            user: Some(sample_user()),
            fail: None,
        };
        let service = ChatServiceImpl::new(chat_repo, room_repo, user_repo);
        let result = service.delete_conversation(1, 1).await;
        assert!(matches!(result, Err(ChatError::ConversationNotFound(1))));
    }
}
