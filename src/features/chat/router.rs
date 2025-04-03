#![allow(unused)]
use salvo::{
    oapi::extract::{JsonBody, PathParam, QueryParam},
    prelude::*,
};

use crate::core::{
    api::salvo_config::DbConnection,
    dtos::{chat::send_message_dto::SendMessageDto, pagination_dto::PaginationDto},
    types::res::failed_response::FailedResponse,
    utils::jwt_utils::JwtUtils,
};

use super::{
    repository::ChatRepositoryImpl,
    service::{ChatService, ChatServiceImpl},
};

#[handler]
async fn set_chat_service(depot: &mut Depot) {
    let pool = depot.obtain::<DbConnection>().unwrap();

    let chat_repository = ChatRepositoryImpl::new(pool.clone().0);
    let chat_service = ChatServiceImpl::new(chat_repository);

    depot.inject(chat_service);
}

pub fn get_chat_router(jwt_utils: JwtUtils) -> Router {
    let router = Router::with_hoop(jwt_utils.auth_middleware())
        .hoop(set_chat_service)
        .path("chats")
        .push(
            Router::with_path("/{meeting_id}")
                .post(create_message)
                .get(get_messages_by_meeting),
        )
        .push(
            Router::with_path("/{message_id}")
                .put(update_message)
                .delete(delete_message),
        )
        .push(Router::with_path("conversations/{meeting_id}").delete(delete_conversation));

    router
}

/// Get messages by room
#[endpoint(tags("chats"))]
async fn get_messages_by_meeting(
    res: &mut Response,
    meeting_id: PathParam<i32>,
    pagination_dto: QueryParam<PaginationDto>,
    depot: &mut Depot,
) {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();

    let pagination_dto = pagination_dto.clone();
    let meeting_id = meeting_id.0;

    let messages = chat_service
        .get_messages_by_meeting(meeting_id, pagination_dto.skip, pagination_dto.limit)
        .await;

    match messages {
        Ok(messages) => {
            res.render(Json(messages));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Send message
#[endpoint(tags("chats"))]
async fn create_message(
    res: &mut Response,
    meeting_id: PathParam<i32>,
    data: JsonBody<SendMessageDto>,
    depot: &mut Depot,
) {
}

/// Update message
#[endpoint(tags("chats"))]
async fn update_message(
    res: &mut Response,
    message_id: PathParam<i32>,
    data: JsonBody<SendMessageDto>,
    depot: &mut Depot,
) {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
}

/// Delete message
#[endpoint(tags("chats"))]
async fn delete_message(res: &mut Response, message_id: PathParam<i32>, depot: &mut Depot) {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
}

/// Delete conversation
#[endpoint(tags("chats"))]
async fn delete_conversation(res: &mut Response, meeting_id: PathParam<i32>, depot: &mut Depot) {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
}
