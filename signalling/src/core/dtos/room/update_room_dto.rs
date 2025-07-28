use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
#[serde(rename_all = "camelCase")]
#[salvo(schema(example = json!(
    {
        "title": "Dev Daily Meeting",
        "password": "123123",
        "avatar": "https://example.com/avatar.png",
        "capacity": 10
    }
)))]
pub struct UpdateRoomDto {
    #[validate(length(min = 3))]
    pub title: Option<String>,

    #[validate(length(min = 6))]
    pub password: Option<String>,

    #[validate(url)]
    pub avatar: Option<String>,

    pub capacity: Option<i32>,
}
