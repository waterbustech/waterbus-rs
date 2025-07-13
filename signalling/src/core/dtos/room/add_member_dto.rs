use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate)]
pub struct AddMemberDto {
    #[serde(rename = "userId")]
    pub user_id: i32,
}
