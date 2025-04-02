use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
pub struct CreateMeetingDto {
    #[validate(length(min = 3))]
    pub title: String,

    #[validate(length(min = 6))]
    pub password: String,
}
