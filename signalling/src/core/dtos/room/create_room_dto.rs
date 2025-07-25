use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

use crate::core::entities::models::{RoomType, StreamingProtocol};

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
#[serde(rename_all = "camelCase")]
#[salvo(schema(example = json!(
    {
        "title": "Dev Daily Meeting",
        "password": "123123",
        "roomType": 0,
        "streamingProtocol": 0,
        "capacity": 10
    }
)))]
pub struct CreateRoomDto {
    #[validate(length(min = 3))]
    pub title: String,
    #[validate(length(min = 6))]
    pub password: Option<String>,
    pub room_type: RoomType,
    pub streaming_protocol: StreamingProtocol,
    pub capacity: Option<i32>,
}
