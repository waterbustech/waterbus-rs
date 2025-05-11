use diesel::{
    ExpressionMethods, JoinOnDsl, NullableExpressionMethods, PgConnection, QueryDsl, RunQueryDsl,
    SelectableHelper,
    dsl::{insert_into, update},
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use salvo::async_trait;

use crate::core::{
    database::schema::{messages, rooms, users},
    entities::models::{Message, MessagesStatusEnum, NewMessage, Room, User},
    types::{
        errors::{chat_error::ChatError, general::GeneralError},
        res::message_response::MessageResponse,
    },
};

#[async_trait]
pub trait ChatRepository: Send + Sync {
    async fn get_messages_by_room(
        &self,
        room_id: i32,
        deleted_at: chrono::NaiveDateTime,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError>;

    async fn get_message_by_id(&self, message_id: i32) -> Result<MessageResponse, ChatError>;

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
    async fn get_messages_by_room(
        &self,
        room_id: i32,
        deleted_at: chrono::NaiveDateTime,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MessageResponse>, ChatError> {
        let mut conn = self.get_conn()?;

        let result = messages::table
            .filter(messages::room_id.eq(room_id))
            .filter(messages::created_at.gt(deleted_at))
            .left_join(rooms::table.on(messages::room_id.eq(rooms::id.nullable())))
            .left_join(users::table.on(messages::created_by_id.eq(users::id.nullable())))
            .select((
                Message::as_select(),
                Option::<Room>::as_select(),
                Option::<User>::as_select(),
            ))
            .order(messages::created_at.desc())
            .offset(skip)
            .limit(limit)
            .load::<(Message, Option<Room>, Option<User>)>(&mut conn)
            .map_err(|_| ChatError::UnexpectedError("Failed to get messages".to_string()))?;

        let response = result
            .into_iter()
            .map(|(message, room, user)| MessageResponse {
                message,
                created_by: user,
                room,
            })
            .collect::<Vec<_>>();

        Ok(response)
    }

    async fn get_message_by_id(&self, message_id: i32) -> Result<MessageResponse, ChatError> {
        let mut conn = self.get_conn()?;

        let result = messages::table
            .filter(messages::id.eq(message_id))
            .left_join(rooms::table.on(messages::room_id.eq(rooms::id.nullable())))
            .left_join(users::table.on(messages::created_by_id.eq(users::id.nullable())))
            .select((
                Message::as_select(),
                Option::<Room>::as_select(),
                Option::<User>::as_select(),
            ))
            .first::<(Message, Option<Room>, Option<User>)>(&mut conn)
            .map_err(|err| match err {
                diesel::result::Error::NotFound => ChatError::MessageNotFound(message_id),
                _ => ChatError::UnexpectedError("Failed to get message".into()),
            })?;

        let (message, room, user) = result;

        Ok(MessageResponse {
            message,
            created_by: user,
            room,
        })
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

        let message_status = MessagesStatusEnum::Inactive as i32;

        let updated_message = update(messages::table)
            .filter(messages::id.eq(message_id))
            .set(messages::status.eq(message_status))
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
