use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Serialize, Deserialize, Validate, ToSchema)]
pub struct OauthRequestDto {
    #[validate(length(min = 1))]
    #[serde(rename = "code")]
    code: String,

    #[validate(length(min = 1))]
    #[serde(rename = "clientId")]
    client_id: String,

    #[validate(length(min = 1))]
    #[serde(rename = "redirectUri")]
    redirect_uri: String,
}
