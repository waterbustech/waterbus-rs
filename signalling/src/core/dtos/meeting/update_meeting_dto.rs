use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
#[salvo(schema(example = json!({"code": 123123, "title": "Dev Daily Meeting", "password": "123123"})))]
pub struct UpdateMeetingDto {
    pub code: i32,

    #[validate(length(min = 3))]
    pub title: Option<String>,

    #[validate(length(min = 6))]
    pub password: Option<String>,

    #[validate(url)]
    pub avatar: Option<String>,
}
