use chrono::{NaiveDateTime, Utc};
use salvo::async_trait;

use crate::{
    core::{
        entities::models::{MessagesStatusEnum, MessagesTypeEnum, NewMessage, Room},
        types::{errors::chat_error::ChatError, responses::message_response::MessageResponse},
    },
    features::{
        room::repository::{RoomRepository, RoomRepositoryImpl},
        user::repository::{UserRepository, UserRepositoryImpl},
    },
};

use super::repository::{ChatRepository, ChatRepositoryImpl};

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
pub struct ChatServiceImpl {
    chat_repository: ChatRepositoryImpl,
    room_repository: RoomRepositoryImpl,
    user_repository: UserRepositoryImpl,
}

impl ChatServiceImpl {
    pub fn new(
        chat_repository: ChatRepositoryImpl,
        room_repository: RoomRepositoryImpl,
        user_repository: UserRepositoryImpl,
    ) -> Self {
        Self {
            chat_repository,
            room_repository,
            user_repository,
        }
    }
}

#[async_trait]
impl ChatService for ChatServiceImpl {
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
            .position(|member| member.member.user_id == Some(user_id));

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
            status: &(MessagesStatusEnum::Active as i32),
            type_: &(MessagesTypeEnum::Default as i32),
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

        if message_response.message.status == MessagesStatusEnum::Inactive as i32 {
            return Err(ChatError::UnexpectedError(
                "Message has been deleted".to_string(),
            ));
        }

        if message_response.message.created_by_id != Some(user_id) {
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

        if message_response.message.status == MessagesStatusEnum::Inactive as i32 {
            return Err(ChatError::UnexpectedError(
                "Message has been deleted".to_string(),
            ));
        }

        if message_response.message.created_by_id != Some(user_id) {
            return Err(ChatError::Forbidden(
                "You not allowed modify message of other users".to_string(),
            ));
        }

        let mut message = message_response.message;

        message.status = MessagesStatusEnum::Inactive as i32;

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
            .position(|member| member.member.user_id == Some(user_id));

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
