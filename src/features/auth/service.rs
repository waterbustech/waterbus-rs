use salvo::async_trait;

use crate::core::{entities::models::User, errors::auth_error::AuthError};

#[async_trait]
trait AuthService: Send + Sync {
    async fn login_with_social(&self) -> Result<User, AuthError>;

    async fn refresh_token(&self) -> Result<String, AuthError>;
}
