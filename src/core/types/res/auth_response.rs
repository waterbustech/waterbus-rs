use serde::{Deserialize, Serialize};

use crate::core::entities::models::User;

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    pub user: Option<User>,
}
