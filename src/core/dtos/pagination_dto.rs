use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

fn default_skip() -> i32 {
    0
}

fn default_limit() -> i32 {
    10
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate)]
pub struct PaginationDto {
    #[serde(default = "default_skip")]
    pub skip: i32,

    #[serde(default = "default_limit")]
    pub limit: i32,
}
