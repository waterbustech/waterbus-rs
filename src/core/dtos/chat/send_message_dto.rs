use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate)]
#[salvo(schema(example = json!({"data": "Hey, morning!"})))]
pub struct SendMessageDto {
    #[validate(length(min = 1))]
    pub data: String,
}
