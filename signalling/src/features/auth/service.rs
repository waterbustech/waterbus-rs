use crate::core::{
    dtos::auth::create_token_dto::CreateTokenDto,
    entities::models::NewUser,
    types::{errors::auth_error::AuthError, responses::auth_response::AuthResponse},
    utils::{id_utils::generate_username, jwt_utils::JwtUtils},
};
use chrono::Utc;
use salvo::async_trait;

use super::repository::AuthRepository;

#[async_trait]
pub trait AuthService: Send + Sync {
    async fn login_with_social(
        &self,
        data: CreateTokenDto,
        jwt_utils: JwtUtils,
    ) -> Result<AuthResponse, AuthError>;

    async fn refresh_token(
        &self,
        jwt_utils: JwtUtils,
        user_id: i32,
    ) -> Result<AuthResponse, AuthError>;
}

#[derive(Debug, Clone)]
pub struct AuthServiceImpl<R: AuthRepository> {
    repository: R,
}

impl<R: AuthRepository> AuthServiceImpl<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl<R: AuthRepository + Send + Sync> AuthService for AuthServiceImpl<R> {
    async fn login_with_social(
        &self,
        data: CreateTokenDto,
        jwt_utils: JwtUtils,
    ) -> Result<AuthResponse, AuthError> {
        let login_dto = data.clone();

        let external_id = login_dto.external_id;

        let user_exists = self.repository.get_user_by_auth_id(external_id).await;

        match user_exists {
            Ok(user) => {
                let token = jwt_utils.clone().generate_token(&user.id.to_string());
                let refresh_token = jwt_utils
                    .clone()
                    .generate_refresh_token(&user.id.to_string());

                let response = AuthResponse {
                    user: Some(user),
                    token,
                    refresh_token,
                };

                Ok(response)
            }
            Err(_) => {
                let now = Utc::now().naive_utc();

                // Create new user
                let new_user = NewUser {
                    full_name: Some(&login_dto.full_name),
                    external_id: &data.external_id,
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
                            token,
                            refresh_token,
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
        let token = jwt_utils.clone().generate_token(&user_id.to_string());
        let refresh_token = jwt_utils
            .clone()
            .generate_refresh_token(&user_id.to_string());

        let response = AuthResponse {
            user: None,
            token,
            refresh_token,
        };

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dtos::auth::create_token_dto::CreateTokenDto;
    use crate::core::entities::models::{NewUser, User};
    use crate::core::types::errors::auth_error::AuthError;
    use chrono::DateTime;

    fn sample_user(id: i32, external_id: &str) -> User {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        User {
            id,
            full_name: Some("Test User".to_string()),
            user_name: "testuser".to_string(),
            bio: Some("bio".to_string()),
            external_id: external_id.to_string(),
            avatar: Some("avatar.png".to_string()),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            last_seen_at: None,
        }
    }

    fn sample_create_token_dto() -> CreateTokenDto {
        CreateTokenDto {
            full_name: "Test User".to_string(),
            external_id: "extid".to_string(),
        }
    }

    // Helper to create a dummy AppEnv for JwtUtils
    fn dummy_app_env() -> crate::core::env::app_env::AppEnv {
        use crate::core::env::app_env::{AppEnv, DbUri, GrpcConfigs, JwtConfig};
        AppEnv {
            group_id: "test-group".to_string(),
            etcd_addr: "localhost:2379".to_string(),
            app_port: 1234,
            client_api_key: "dummy".to_string(),
            db_uri: DbUri("dummy_db_uri".to_string()),
            redis_uris: vec!["redis://localhost:6379".to_string()],
            jwt: JwtConfig {
                jwt_token: "secret".to_string(),
                refresh_token: "refresh_secret".to_string(),
                token_expires_in_seconds: 3600,
                refresh_token_expires_in_seconds: 7200,
            },
            grpc_configs: GrpcConfigs {
                sfu_host: "localhost".to_string(),
                sfu_port: 1,
                dispatcher_host: "localhost".to_string(),
                dispatcher_port: 2,
            },
            tls_enabled: false,
            api_suffix: "busapi".to_string(),
        }
    }

    struct MockAuthRepository {
        pub user_exists: Option<User>,
        pub create_user_result: Result<User, AuthError>,
    }

    #[async_trait]
    impl AuthRepository for MockAuthRepository {
        async fn create_user(&self, _user: NewUser<'_>) -> Result<User, AuthError> {
            self.create_user_result.clone()
        }
        async fn get_user_by_id(&self, id: i32) -> Result<User, AuthError> {
            Err(AuthError::UserNotFound(id))
        }
        async fn get_user_by_auth_id(&self, _external_id: String) -> Result<User, AuthError> {
            match &self.user_exists {
                Some(user) => Ok(user.clone()),
                None => Err(AuthError::UserNotFound(0)),
            }
        }
        async fn get_user_by_user_name(&self, _username: String) -> Result<User, AuthError> {
            Err(AuthError::UserNotFound(0))
        }
    }

    #[tokio::test]
    async fn test_login_with_social_user_exists() {
        let user = sample_user(1, "extid");
        let repo = MockAuthRepository {
            user_exists: Some(user.clone()),
            create_user_result: Ok(user.clone()), // not used
        };
        let service = AuthServiceImpl::new(repo);
        let jwt_utils = JwtUtils::new(dummy_app_env());
        let dto = sample_create_token_dto();
        let result = service
            .login_with_social(dto, jwt_utils.clone())
            .await
            .unwrap();
        assert_eq!(result.user.unwrap().id, 1);
        assert!(!result.token.is_empty());
        assert!(!result.refresh_token.is_empty());
    }

    #[tokio::test]
    async fn test_login_with_social_user_not_exists_create_success() {
        let user = sample_user(2, "extid");
        let repo = MockAuthRepository {
            user_exists: None,
            create_user_result: Ok(user.clone()),
        };
        let service = AuthServiceImpl::new(repo);
        let jwt_utils = JwtUtils::new(dummy_app_env());
        let dto = sample_create_token_dto();
        let result = service
            .login_with_social(dto, jwt_utils.clone())
            .await
            .unwrap();
        assert_eq!(result.user.unwrap().id, 2);
        assert!(!result.token.is_empty());
        assert!(!result.refresh_token.is_empty());
    }

    #[tokio::test]
    async fn test_login_with_social_user_not_exists_create_fail() {
        let repo = MockAuthRepository {
            user_exists: None,
            create_user_result: Err(AuthError::UnexpectedError(
                "Failed to create new user".to_string(),
            )),
        };
        let service = AuthServiceImpl::new(repo);
        let jwt_utils = JwtUtils::new(dummy_app_env());
        let dto = sample_create_token_dto();
        let result = service.login_with_social(dto, jwt_utils.clone()).await;
        assert!(matches!(result, Err(AuthError::UnexpectedError(_))));
    }

    #[tokio::test]
    async fn test_refresh_token() {
        let repo = MockAuthRepository {
            user_exists: None,
            create_user_result: Err(AuthError::UnexpectedError("not used".to_string())),
        };
        let service = AuthServiceImpl::new(repo);
        let jwt_utils = JwtUtils::new(dummy_app_env());
        let result = service.refresh_token(jwt_utils.clone(), 42).await.unwrap();
        assert!(result.user.is_none());
        assert!(!result.token.is_empty());
        assert!(!result.refresh_token.is_empty());
    }
}
