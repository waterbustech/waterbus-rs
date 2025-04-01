use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdateMeetingDto {
    code: i32,

    #[validate(length(min = 3))]
    title: Option<String>,

    #[validate(length(min = 6))]
    password: Option<String>,

    #[validate(url)]
    avatar: Option<String>,
}
