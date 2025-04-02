use diesel::{
    ExpressionMethods, JoinOnDsl, NullableExpressionMethods, PgConnection, QueryDsl, RunQueryDsl,
    SelectableHelper,
    dsl::{insert_into, update},
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use salvo::async_trait;

use crate::core::{
    database::schema::{meetings, messages, users},
    entities::models::{Meeting, Message, MessagesStatusEnum, NewMessage, User},
    types::{
        errors::{chat_error::ChatError, general::GeneralError},
        res::message_response::MessageResponse,
    },
};

#[async_trait]
pub trait ChatRepository: Send + Sync {
    async fn get_messages_by_meeting(
        &self,
        meeting_id: i32,
        deleted_at: chrono::NaiveDateTime,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError>;

    async fn create_message(&self, message: NewMessage<'_>) -> Result<Message, ChatError>;

    async fn update_message(&self, message: Message) -> Result<Message, ChatError>;

    async fn delete_message_by_id(&self, message_id: i32) -> Result<Message, ChatError>;
}

#[derive(Debug, Clone)]
pub struct ChatRepositoryImpl {
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl ChatRepositoryImpl {
    pub fn new(pool: Pool<ConnectionManager<PgConnection>>) -> Self {
        Self { pool }
    }

    fn get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, GeneralError> {
        self.pool.get().map_err(|_| GeneralError::DbConnectionError)
    }
}

#[async_trait]
impl ChatRepository for ChatRepositoryImpl {
    async fn get_messages_by_meeting(
        &self,
        meeting_id: i32,
        deleted_at: chrono::NaiveDateTime,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError> {
        let mut conn = self.get_conn()?;

        let messages = messages::table
            .inner_join(meetings::table.on(messages::meetingId.eq(meetings::id.nullable())))
            .inner_join(users::table.on(messages::createdById.eq(users::id.nullable())))
            .filter(messages::meetingId.eq(meeting_id))
            .filter(messages::createdAt.gt(deleted_at))
            .order(messages::createdAt.desc())
            .offset(skip)
            .limit(limit)
            .load::<(Message, Meeting, User)>(&mut conn);

        match messages {
            Ok(messages) => {
                let response = messages
                    .into_iter()
                    .map(|(message, meeting, user)| MessageResponse {
                        id: message.id,
                        data: message.data,
                        type_: message.type_,
                        status: message.status,
                        created_at: message.createdAt,
                        updated_at: message.updatedAt,
                        deleted_at: message.deletedAt,
                        created_by: Some(user),
                        meeting: Some(meeting),
                    })
                    .collect::<Vec<_>>();

                Ok(response)
            }
            Err(_) => Err(ChatError::UnexpectedError(
                "Failed to get messages".to_string(),
            )),
        }
    }

    async fn create_message(&self, message: NewMessage<'_>) -> Result<Message, ChatError> {
        let mut conn = self.get_conn()?;

        let new_message = insert_into(messages::table)
            .values(&message)
            .returning(Message::as_select())
            .get_result(&mut conn);

        match new_message {
            Ok(message) => Ok(message),
            Err(_) => Err(ChatError::UnexpectedError(
                "Failed to create new message".to_string(),
            )),
        }
    }

    async fn update_message(&self, message: Message) -> Result<Message, ChatError> {
        let mut conn = self.get_conn()?;

        let updated_message = update(messages::table)
            .filter(messages::id.eq(message.id))
            .set(messages::data.eq(message.data))
            .returning(Message::as_select())
            .get_result(&mut conn);

        match updated_message {
            Ok(message) => Ok(message),
            Err(_) => Err(ChatError::UnexpectedError(
                "Failed to update message".to_string(),
            )),
        }
    }

    async fn delete_message_by_id(&self, message_id: i32) -> Result<Message, ChatError> {
        let mut conn = self.get_conn()?;

        let updated_message = update(messages::table)
            .filter(messages::id.eq(message_id))
            .set(messages::status.eq(MessagesStatusEnum::Inactive))
            .returning(Message::as_select())
            .get_result(&mut conn);

        match updated_message {
            Ok(message) => Ok(message),
            Err(_) => Err(ChatError::UnexpectedError(
                "Failed to update message".to_string(),
            )),
        }
    }
}
