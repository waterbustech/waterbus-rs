use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

fn default_skip() -> i64 {
    0
}

fn default_limit() -> i64 {
    10
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate, Clone)]
pub struct PaginationDto {
    #[serde(default = "default_skip")]
    pub skip: i64,

    #[serde(default = "default_limit")]
    pub limit: i64,
}
