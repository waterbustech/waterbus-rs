use salvo::async_trait;

use crate::core::{
    entities::models::{Meeting, Message},
    types::{errors::chat_error::ChatError, res::message_response::MessageResponse},
};

use super::repository::ChatRepositoryImpl;

#[async_trait]
pub trait ChatService: Send + Sync {
    async fn get_messages_by_meeting(
        &self,
        meeting_id: i32,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError>;

    async fn create_message(&self, message: Message) -> Result<Message, ChatError>;

    async fn update_message(&self, message: Message) -> Result<Message, ChatError>;

    async fn delete_message_by_id(&self, message_id: i32) -> Result<Message, ChatError>;

    async fn delete_conversation(&self, conversation_id: i32) -> Result<Meeting, ChatError>;
}

#[derive(Debug, Clone)]
pub struct ChatServiceImpl {
    repository: ChatRepositoryImpl,
}

impl ChatServiceImpl {
    pub fn new(repository: ChatRepositoryImpl) -> Self {
        Self {
            repository: repository,
        }
    }
}

#[async_trait]
impl ChatService for ChatServiceImpl {
    async fn get_messages_by_meeting(
        &self,
        meeting_id: i32,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError> {
        // let messages = self
        //     .repository
        //     .get_messages_by_meeting(meeting_id, deleted_at, skip, limit)
        //     .await;
        todo!()
    }

    async fn create_message(&self, message: Message) -> Result<Message, ChatError> {
        todo!()
    }

    async fn update_message(&self, message: Message) -> Result<Message, ChatError> {
        todo!()
    }

    async fn delete_message_by_id(&self, message_id: i32) -> Result<Message, ChatError> {
        todo!()
    }

    async fn delete_conversation(&self, conversation_id: i32) -> Result<Meeting, ChatError> {
        todo!()
    }
}
