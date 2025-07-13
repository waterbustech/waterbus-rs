use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
#[salvo(schema(example = json!({"fullName": "Kai", "bio": "waterbus"})))]
pub struct UpdateUserDto {
    #[validate(length(min = 1))]
    #[serde(rename = "fullName")]
    pub full_name: String,

    #[validate(url)]
    pub avatar: Option<String>,

    pub bio: Option<String>,
}
