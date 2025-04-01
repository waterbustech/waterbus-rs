use salvo::{oapi::extract::PathParam, prelude::*};

use crate::core::utils::jwt_utils::JwtUtils;

pub fn get_chat_router(jwt_utils: JwtUtils) -> Router {
    let router = Router::with_hoop(jwt_utils.auth_middleware())
        .path("chats")
        .post(create_message)
        .get(get_messages_by_meeting)
        .put(update_message)
        .delete(delete_message)
        .push(Router::with_path("conversations").delete(delete_conversation));

    router
}

/// Get messages by room
#[endpoint(tags("chats"))]
async fn get_messages_by_meeting(res: &mut Response, meeting_id: PathParam<i32>) {}

/// Send message
#[endpoint(tags("chats"))]
async fn create_message(res: &mut Response, meeting_id: PathParam<i32>) {}

/// Update message
#[endpoint(tags("chats"))]
async fn update_message(res: &mut Response, message_id: PathParam<i32>) {}

/// Delete message
#[endpoint(tags("chats"))]
async fn delete_message(res: &mut Response, message_id: PathParam<i32>) {}

/// Delete conversation
#[endpoint(tags("chats"))]
async fn delete_conversation(res: &mut Response, meeting_id: PathParam<i32>) {}
