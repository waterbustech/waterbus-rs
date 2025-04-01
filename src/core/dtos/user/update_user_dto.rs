use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdateUserDto {
    #[validate(length(min = 1))]
    #[serde(rename = "fullName")]
    full_name: String,

    #[validate(url)]
    avatar: Option<String>,

    bio: Option<String>,
}
