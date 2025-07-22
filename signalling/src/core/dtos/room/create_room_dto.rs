use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

use crate::core::entities::models::{RoomType, StreamingProtocol};

fn default_room_type() -> RoomType {
    RoomType::Conferencing
}

fn default_streaming_protocol() -> StreamingProtocol {
    StreamingProtocol::SFU
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
#[salvo(schema(example = json!({"title": "Dev Daily Meeting", "password": "123123", "room_type": 0})))]
pub struct CreateRoomDto {
    #[validate(length(min = 3))]
    pub title: String,

    #[validate(length(min = 6))]
    pub password: Option<String>,

    #[serde(default = "default_room_type")]
    pub room_type: RoomType,

    #[serde(default = "default_streaming_protocol")]
    pub streaming_protocol: StreamingProtocol,

    pub capacity: Option<i32>,
}
