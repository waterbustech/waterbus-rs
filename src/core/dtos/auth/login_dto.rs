use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
#[salvo(schema(example = json!({"fullName": "Kai", "googleId": "lambiengcode"})))]
pub struct LoginDto {
    #[validate(length(min = 1))]
    #[serde(rename = "fullName")]
    pub full_name: String,

    #[serde(rename = "googleId")]
    pub google_id: Option<String>,

    #[serde(rename = "githubId")]
    pub github_id: Option<String>,
}
