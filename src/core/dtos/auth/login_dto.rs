use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate)]
pub struct LoginDto {
    #[validate(length(min = 1))]
    #[serde(rename = "fullName")]
    full_name: String,

    #[serde(rename = "googleId")]
    google_id: Option<String>,

    #[serde(rename = "githubId")]
    github_id: Option<String>,
}
