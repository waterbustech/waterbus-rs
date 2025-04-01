use salvo::async_trait;

use crate::core::{
    entities::models::{Meeting, Message},
    types::errors::chat_error::ChatError,
};

#[async_trait]
pub trait ChatService: Send + Sync {
    async fn get_messages_by_meeting(&self, meeting_id: i32) -> Result<Vec<Message>, ChatError>;

    async fn create_message(&self, message: Message) -> Result<Message, ChatError>;

    async fn update_message(&self, message: Message) -> Result<Message, ChatError>;

    async fn delete_message_by_id(&self, message_id: i32) -> Result<Message, ChatError>;

    async fn delete_conversation(&self, conversation_id: i32) -> Result<Meeting, ChatError>;
}
