use async_channel::Sender;
use salvo::{
    oapi::extract::{JsonBody, PathParam},
    prelude::*,
};

use crate::core::{
    dtos::{chat::send_message_dto::SendMessageDto, common::pagination_dto::PaginationDto},
    types::{
        app_channel::AppEvent,
        errors::chat_error::ChatError,
        responses::{
            list_message_response::ListMessageResponse, message_response::MessageResponse,
            room_response::RoomResponse,
        },
    },
    utils::jwt_utils::JwtUtils,
};

use super::service::{ChatService, ChatServiceImpl};

pub fn get_chat_router(jwt_utils: JwtUtils) -> Router {
    Router::with_hoop(jwt_utils.auth_middleware())
        .path("chats")
        .push(
            Router::with_path("/{room_id}")
                .post(create_message)
                .get(get_messages_by_room),
        )
        .push(
            Router::with_path("/{message_id}")
                .put(update_message)
                .delete(delete_message),
        )
        .push(Router::with_path("conversations/{room_id}").delete(delete_conversation))
}

/// Get messages by room
#[endpoint(tags("chats"), status_codes(200, 400, 500))]
async fn get_messages_by_room(
    _res: &mut Response,
    room_id: PathParam<i32>,
    pagination_dto: PaginationDto,
    depot: &mut Depot,
) -> Result<ListMessageResponse, ChatError> {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let pagination_dto = pagination_dto.clone();
    let room_id = room_id.0;

    let messages = chat_service
        .get_messages_by_room(
            room_id,
            user_id.parse().unwrap(),
            pagination_dto.skip,
            pagination_dto.limit,
        )
        .await?;

    Ok(ListMessageResponse { messages })
}

/// Send message
#[endpoint(tags("chats"), status_codes(201, 400, 403, 404, 500))]
async fn create_message(
    _res: &mut Response,
    room_id: PathParam<i32>,
    data: JsonBody<SendMessageDto>,
    depot: &mut Depot,
) -> Result<MessageResponse, ChatError> {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
    let app_channel_tx = depot.obtain::<Sender<AppEvent>>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let data = data.0.data;
    let room_id = room_id.into_inner();

    let message = chat_service
        .create_message(room_id, user_id.parse().unwrap(), &data)
        .await?;

    let _ = app_channel_tx
        .send(AppEvent::SendMessage(message.clone()))
        .await;

    Ok(message)
}

/// Update message
#[endpoint(tags("chats"), status_codes(200, 400, 403, 404, 500))]
async fn update_message(
    _res: &mut Response,
    message_id: PathParam<i32>,
    data: JsonBody<SendMessageDto>,
    depot: &mut Depot,
) -> Result<MessageResponse, ChatError> {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
    let app_channel_tx = depot.obtain::<Sender<AppEvent>>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let data = data.0.data;
    let message_id = message_id.into_inner();

    let message = chat_service
        .update_message(message_id, user_id.parse().unwrap(), &data)
        .await?;

    let _ = app_channel_tx
        .send(AppEvent::UpdateMessage(message.clone()))
        .await;

    Ok(message)
}

/// Delete message
#[endpoint(tags("chats"), status_codes(200, 400, 403, 404, 500))]
async fn delete_message(
    _res: &mut Response,
    message_id: PathParam<i32>,
    depot: &mut Depot,
) -> Result<MessageResponse, ChatError> {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
    let app_channel_tx = depot.obtain::<Sender<AppEvent>>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let message_id = message_id.into_inner();

    let message = chat_service
        .delete_message_by_id(message_id, user_id.parse().unwrap())
        .await?;

    let _ = app_channel_tx
        .send(AppEvent::DeleteMessage(message.clone()))
        .await;

    Ok(message)
}

/// Delete conversation
#[endpoint(tags("chats"), status_codes(200, 400, 403, 404, 500))]
async fn delete_conversation(
    _res: &mut Response,
    room_id: PathParam<i32>,
    depot: &mut Depot,
) -> Result<RoomResponse, ChatError> {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let room_id = room_id.into_inner();

    let room = chat_service
        .delete_conversation(room_id, user_id.parse().unwrap())
        .await?;

    Ok(RoomResponse {
        room,
        members: vec![],
        participants: vec![],
        latest_message: None,
    })
}
