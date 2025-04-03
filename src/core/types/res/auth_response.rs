use serde::{Deserialize, Serialize};

use crate::core::entities::models::User;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub token: String,
    pub refresh_token: String,
    pub user: Option<User>,
}
