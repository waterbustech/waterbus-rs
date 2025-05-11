use serde::Serialize;

use crate::core::entities::models::{Message, Room, User};

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageResponse {
    #[serde(flatten)]
    pub message: Message,
    pub created_by: Option<User>,
    pub room: Option<Room>,
}
