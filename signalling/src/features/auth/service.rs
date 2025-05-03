use crate::core::{
    dtos::auth::login_dto::LoginDto,
    entities::models::NewUser,
    types::{errors::auth_error::AuthError, res::auth_response::AuthResponse},
    utils::{id_utils::generate_username, jwt_utils::JwtUtils},
};
use chrono::Utc;
use salvo::async_trait;

use super::repository::{AuthRepository, AuthRepositoryImpl};

#[async_trait]
pub trait AuthService: Send + Sync {
    async fn login_with_social(
        &self,
        data: LoginDto,
        jwt_utils: JwtUtils,
    ) -> Result<AuthResponse, AuthError>;

    async fn refresh_token(
        &self,
        jwt_utils: JwtUtils,
        user_id: i32,
    ) -> Result<AuthResponse, AuthError>;
}

#[derive(Debug, Clone)]
pub struct AuthServiceImpl {
    repository: AuthRepositoryImpl,
}

impl AuthServiceImpl {
    pub fn new(repository: AuthRepositoryImpl) -> Self {
        Self {
            repository: repository,
        }
    }
}

#[async_trait]
impl AuthService for AuthServiceImpl {
    async fn login_with_social(
        &self,
        data: LoginDto,
        jwt_utils: JwtUtils,
    ) -> Result<AuthResponse, AuthError> {
        let login_dto = data.clone();

        let google_id = login_dto.google_id.as_deref();
        let custom_id = login_dto.custom_id.as_deref();

        let user_exists = self
            .repository
            .get_user_by_auth_id(google_id, custom_id)
            .await;

        match user_exists {
            Ok(user) => {
                let token = jwt_utils.clone().generate_token(&user.id.to_string());
                let refresh_token = jwt_utils
                    .clone()
                    .generate_refresh_token(&user.id.to_string());

                let response = AuthResponse {
                    user: Some(user),
                    token: token,
                    refresh_token: refresh_token,
                };

                Ok(response)
            }
            Err(_) => {
                let now = Utc::now().naive_utc();

                // Create new user
                let new_user = NewUser {
                    full_name: Some(&login_dto.full_name),
                    google_id: google_id,
                    custom_id: data.custom_id.as_ref().map(|s: &String| s.as_str()),
                    user_name: &generate_username(),
                    created_at: now,
                    updated_at: now,
                    bio: None,
                    avatar: None,
                };

                let new_user = self.repository.create_user(new_user).await;

                match new_user {
                    Ok(user) => {
                        let token = jwt_utils.clone().generate_token(&user.id.to_string());
                        let refresh_token = jwt_utils
                            .clone()
                            .generate_refresh_token(&user.id.to_string());

                        let response = AuthResponse {
                            user: Some(user),
                            token: token,
                            refresh_token: refresh_token,
                        };

                        Ok(response)
                    }
                    Err(_) => Err(AuthError::UnexpectedError(
                        "Failed to create new user".to_string(),
                    )),
                }
            }
        }
    }

    async fn refresh_token(
        &self,
        jwt_utils: JwtUtils,
        user_id: i32,
    ) -> Result<AuthResponse, AuthError> {
        println!("user_id: {}", user_id);
        let token = jwt_utils.clone().generate_token(&user_id.to_string());
        let refresh_token = jwt_utils
            .clone()
            .generate_refresh_token(&user_id.to_string());

        let response = AuthResponse {
            user: None,
            token: token,
            refresh_token: refresh_token,
        };

        Ok(response)
    }
}
