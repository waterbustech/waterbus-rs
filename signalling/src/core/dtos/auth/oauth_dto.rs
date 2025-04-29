use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Serialize, Deserialize, Validate, ToSchema)]
pub struct OauthRequestDto {
    #[validate(length(min = 1))]
    #[serde(rename = "code")]
    pub code: String,

    #[validate(length(min = 1))]
    #[serde(rename = "clientId")]
    pub client_id: String,

    #[validate(length(min = 1))]
    #[serde(rename = "redirectUri")]
    pub redirect_uri: String,
}
