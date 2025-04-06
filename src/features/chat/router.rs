use salvo::{
    oapi::extract::{JsonBody, PathParam},
    prelude::*,
};

use crate::core::{
    dtos::{chat::send_message_dto::SendMessageDto, pagination_dto::PaginationDto},
    types::res::failed_response::FailedResponse,
    utils::jwt_utils::JwtUtils,
};

use super::service::{ChatService, ChatServiceImpl};

pub fn get_chat_router(jwt_utils: JwtUtils) -> Router {
    let router = Router::with_hoop(jwt_utils.auth_middleware())
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
    pagination_dto: PaginationDto,
    depot: &mut Depot,
) {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let pagination_dto = pagination_dto.clone();
    let meeting_id = meeting_id.0;

    let messages = chat_service
        .get_messages_by_meeting(
            meeting_id,
            user_id.parse().unwrap(),
            pagination_dto.skip,
            pagination_dto.limit,
        )
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
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let data = data.0.data;
    let meeting_id = meeting_id.into_inner();

    let message = chat_service
        .create_message(meeting_id, user_id.parse().unwrap(), &data)
        .await;

    match message {
        Ok(message) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(message));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
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
    let user_id = depot.get::<String>("user_id").unwrap();
    let data = data.0.data;
    let message_id = message_id.into_inner();

    let message = chat_service
        .update_message(message_id, user_id.parse().unwrap(), &data)
        .await;

    match message {
        Ok(message) => {
            res.render(Json(message));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Delete message
#[endpoint(tags("chats"))]
async fn delete_message(res: &mut Response, message_id: PathParam<i32>, depot: &mut Depot) {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let message_id = message_id.into_inner();

    let message = chat_service
        .delete_message_by_id(message_id, user_id.parse().unwrap())
        .await;

    match message {
        Ok(message) => {
            res.render(Json(message));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Delete conversation
#[endpoint(tags("chats"))]
async fn delete_conversation(res: &mut Response, meeting_id: PathParam<i32>, depot: &mut Depot) {
    let chat_service = depot.obtain::<ChatServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let meeting_id = meeting_id.into_inner();

    let meeting = chat_service
        .delete_conversation(meeting_id, user_id.parse().unwrap())
        .await;

    match meeting {
        Ok(meeting) => {
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}
