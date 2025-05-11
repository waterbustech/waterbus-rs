use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate)]
pub struct JoinRoomDto {
    #[validate(length(min = 6))]
    pub password: Option<String>,
}
