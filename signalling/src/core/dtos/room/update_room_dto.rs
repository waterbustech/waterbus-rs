use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

use crate::core::entities::models::{RoomType, StreamingProtocol};

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
#[salvo(schema(example = json!({"title": "Dev Daily Meeting", "password": "123123"})))]
pub struct UpdateRoomDto {
    #[validate(length(min = 3))]
    pub title: Option<String>,

    #[validate(length(min = 6))]
    pub password: Option<String>,

    #[validate(url)]
    pub avatar: Option<String>,

    pub room_type: Option<RoomType>,

    pub streaming_protocol: Option<StreamingProtocol>,

    pub capacity: Option<i32>,
}
