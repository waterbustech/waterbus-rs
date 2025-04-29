use salvo::oapi::ToParameters;
use serde::{Deserialize, Serialize};
use validator_derive::Validate;

fn default_page() -> i64 {
    1 // Default page is 1
}

fn default_per_page() -> i64 {
    10 // Default per_page is 10
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone, ToParameters)]
#[salvo(parameters(default_parameter_in = Query))]
pub struct PageRequestDto {
    #[serde(default = "default_page")]
    pub page: i64,

    #[serde(default = "default_per_page")]
    pub per_page: i64,
}
