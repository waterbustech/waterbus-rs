use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
#[serde(rename_all = "camelCase")]
#[salvo(schema(example = json!({"fullName": "Kai", "externalId": "kai@waterbus"})))]
pub struct CreateTokenDto {
    #[validate(length(min = 1))]
    pub full_name: String,

    #[validate(length(min = 1))]
    pub external_id: String,
}
