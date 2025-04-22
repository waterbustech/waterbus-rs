use chrono::{NaiveDateTime, Utc};
use salvo::async_trait;

use crate::{
    core::{
        entities::models::{Meeting, MembersStatusEnum, Message, MessagesStatusEnum, MessagesTypeEnum, NewMessage},
        types::{errors::chat_error::ChatError, res::message_response::MessageResponse},
    },
    features::{
        meeting::repository::{MeetingRepository, MeetingRepositoryImpl},
        user::repository::{UserRepository, UserRepositoryImpl},
    },
};

use super::repository::{ChatRepository, ChatRepositoryImpl};

#[async_trait]
pub trait ChatService: Send + Sync {
    async fn get_messages_by_meeting(
        &self,
        meeting_id: i32,
        user_id: i32,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError>;

    async fn create_message(
        &self,
        meeting_id: i32,
        user_id: i32,
        data: &str,
    ) -> Result<MessageResponse, ChatError>;

    async fn update_message(
        &self,
        message_id: i32,
        user_id: i32,
        data: &str,
    ) -> Result<Message, ChatError>;

    async fn delete_message_by_id(
        &self,
        message_id: i32,
        user_id: i32,
    ) -> Result<Message, ChatError>;

    async fn delete_conversation(
        &self,
        conversation_id: i32,
        user_id: i32,
    ) -> Result<Meeting, ChatError>;

    async fn update_latest_message_created_at(
        &self,
        meeting: Meeting,
        now: NaiveDateTime,
        latest_mesage_id: Option<i32>,
    );
}

#[derive(Debug, Clone)]
pub struct ChatServiceImpl {
    chat_repository: ChatRepositoryImpl,
    meeting_repository: MeetingRepositoryImpl,
    user_repository: UserRepositoryImpl,
}

impl ChatServiceImpl {
    pub fn new(
        chat_repository: ChatRepositoryImpl,
        meeting_repository: MeetingRepositoryImpl,
        user_repository: UserRepositoryImpl,
    ) -> Self {
        Self {
            chat_repository,
            meeting_repository,
            user_repository,
        }
    }
}

#[async_trait]
impl ChatService for ChatServiceImpl {
    async fn get_messages_by_meeting(
        &self,
        meeting_id: i32,
        user_id: i32,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError> {
        let meeting = self
            .meeting_repository
            .get_meeting_by_id(meeting_id)
            .await
            .map_err(|_| ChatError::ConversationNotFound(meeting_id))?;

        let is_member = meeting.members.iter().any(|member| {
            member.member.user_id == Some(user_id)
                && member.member.status != MembersStatusEnum::Inviting as i32
        });

        if !is_member {
            return Err(ChatError::Forbidden(
                "You not allowed get messages from room that you not stay in there".to_string(),
            ));
        }

        let index_of_user = meeting.members.iter().position(|member| {
            member.member.user_id == Some(user_id)
                && member.member.status != MembersStatusEnum::Inviting as i32
        });

        let deleted_at = match index_of_user {
            Some(index) => {
                let member = &meeting.members[index].member;
                member.soft_deleted_at.unwrap_or(meeting.meeting.created_at)
            }
            None => meeting.meeting.created_at,
        };

        let messages = self
            .chat_repository
            .get_messages_by_meeting(meeting_id, deleted_at, skip, limit)
            .await?;

        Ok(messages)
    }

    async fn create_message(
        &self,
        meeting_id: i32,
        user_id: i32,
        data: &str,
    ) -> Result<MessageResponse, ChatError> {
        let user = self
            .user_repository
            .get_user_by_id(user_id)
            .await
            .map_err(|_| ChatError::MemberNotFound(user_id))?;

        let meeting = self
            .meeting_repository
            .get_meeting_by_id(meeting_id)
            .await
            .map_err(|_| ChatError::ConversationNotFound(meeting_id))?;

        let index_of_member = meeting
            .members
            .iter()
            .position(|member| member.member.user_id == Some(user_id));

        if let Some(index) = index_of_member {
            let mut member = meeting.members[index].member.clone();

            if member.status == MembersStatusEnum::Inviting as i32 {
                return Err(ChatError::Forbidden(
                    "User is not accept invitation".to_string(),
                ));
            } else if member.status == MembersStatusEnum::Invisible as i32 {
                member.status = MembersStatusEnum::Joined as i32;
                let _ = self.meeting_repository.update_member(member).await;
            }
        } else {
            return Err(ChatError::MemberNotFound(user_id));
        }

        let now = Utc::now().naive_utc();

        let new_message = NewMessage {
            data,
            created_by_id: Some(&user_id),
            meeting_id: Some(&meeting_id),
            status: &(MessagesStatusEnum::Active as i32),
            type_: &(MessagesTypeEnum::Default as i32),
            created_at: now,
            updated_at: now,
        };

        let new_message = self.chat_repository.create_message(new_message).await?;

        self.update_latest_message_created_at(meeting.meeting.clone(), now, Some(new_message.id))
            .await;

        Ok(MessageResponse {
            message: new_message,
            created_by: Some(user),
            meeting: Some(meeting.meeting.clone()),
        })
    }

    async fn update_message(
        &self,
        message_id: i32,
        user_id: i32,
        data: &str,
    ) -> Result<Message, ChatError> {
        let message = self.chat_repository.get_message_by_id(message_id).await?;
        let meeting = message.meeting.unwrap();

        if message.message.status == MessagesStatusEnum::Inactive as i32 {
            return Err(ChatError::UnexpectedError(
                "Message has been deleted".to_string(),
            ));
        }

        if message.message.created_by_id != Some(user_id) {
            return Err(ChatError::Forbidden(
                "You not allowed modify message of other users".to_string(),
            ));
        }

        let now = Utc::now().naive_utc();
        let mut message = message.message;

        self.update_latest_message_created_at(meeting, now, None)
            .await;

        message.data = data.to_string();
        message.updated_at = now;

        let message = self.chat_repository.update_message(message).await?;

        Ok(message)
    }

    async fn delete_message_by_id(
        &self,
        message_id: i32,
        user_id: i32,
    ) -> Result<Message, ChatError> {
        let message = self.chat_repository.get_message_by_id(message_id).await?;

        if message.message.status == MessagesStatusEnum::Inactive as i32 {
            return Err(ChatError::UnexpectedError(
                "Message has been deleted".to_string(),
            ));
        }

        if message.message.created_by_id != Some(user_id) {
            return Err(ChatError::Forbidden(
                "You not allowed modify message of other users".to_string(),
            ));
        }

        let mut message = message.message;

        message.status = MessagesStatusEnum::Inactive as i32;

        let message = self.chat_repository.update_message(message).await?;

        Ok(message)
    }

    async fn delete_conversation(
        &self,
        conversation_id: i32,
        user_id: i32,
    ) -> Result<Meeting, ChatError> {
        let meeting = self
            .meeting_repository
            .get_meeting_by_id(conversation_id)
            .await
            .map_err(|_| ChatError::ConversationNotFound(conversation_id))?;

        let index_of_member = meeting
            .members
            .iter()
            .position(|member| member.member.user_id == Some(user_id));

        match index_of_member {
            Some(index) => {
                let now = Utc::now().naive_utc();
                let mut member = meeting.members[index].member.clone();

                member.soft_deleted_at = Some(now);
                member.status = MembersStatusEnum::Invisible as i32;

                let _ = self
                    .meeting_repository
                    .update_member(member)
                    .await
                    .map_err(|_| ChatError::UnexpectedError("".to_string()))?;

                Ok(meeting.meeting)
            }
            None => Err(ChatError::ConversationNotFound(user_id)),
        }
    }

    async fn update_latest_message_created_at(
        &self,
        meeting: Meeting,
        now: NaiveDateTime,
        latest_mesage_id: Option<i32>,
    ) {
        let mut meeting = meeting.clone();

        meeting.latest_message_created_at = Some(now);

        if let Some(id) = latest_mesage_id {
            meeting.latest_message_id = Some(id);
        }

        let _ = self.meeting_repository.update_meeting(meeting).await;
    }
}
