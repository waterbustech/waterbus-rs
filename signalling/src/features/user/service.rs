use salvo::async_trait;

use crate::core::{
    dtos::user::update_user_dto::UpdateUserDto, entities::models::User,
    types::errors::user_error::UserError,
};

use super::repository::UserRepository;

#[async_trait]
pub trait UserService: Send + Sync {
    async fn get_user_by_id(&self, user_id: i32) -> Result<User, UserError>;
    async fn update_user(&self, user_id: i32, data: UpdateUserDto) -> Result<User, UserError>;
    async fn check_username_exists(&self, username: &str) -> bool;
    async fn update_username(&self, user_id: i32, username: &str) -> Result<User, UserError>;
}

// Change struct definition to be generic
pub struct UserServiceImpl<R: UserRepository> {
    repository: R,
}

// Update constructor
impl<R: UserRepository> UserServiceImpl<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl<R: UserRepository + Send + Sync> UserService for UserServiceImpl<R> {
    async fn get_user_by_id(&self, user_id: i32) -> Result<User, UserError> {
        self.repository.get_user_by_id(user_id).await
    }

    async fn update_user(&self, user_id: i32, data: UpdateUserDto) -> Result<User, UserError> {
        let new_user_info = data.clone();
        let user = self.repository.get_user_by_id(user_id).await;

        match user {
            Ok(user) => {
                let mut updated_user = user.clone();

                updated_user.full_name = Some(new_user_info.full_name);

                if let Some(avatar) = new_user_info.avatar {
                    updated_user.avatar = Some(avatar);
                }

                if let Some(bio) = new_user_info.bio {
                    updated_user.bio = Some(bio);
                }

                self.repository.update_user(updated_user).await
            }
            Err(err) => Err(err),
        }
    }

    async fn check_username_exists(&self, username: &str) -> bool {
        let user_name = self.repository.get_username(username).await;

        return user_name.is_ok();
    }

    async fn update_username(&self, user_id: i32, username: &str) -> Result<User, UserError> {
        self.repository.update_username(user_id, username).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dtos::user::update_user_dto::UpdateUserDto;
    use crate::core::entities::models::User;
    use crate::core::types::errors::user_error::UserError;
    use chrono::DateTime;

    struct MockUserRepository {
        pub user: Option<User>,
        pub username_exists: bool,
        pub update_user_result: Option<User>,
        pub update_username_result: Option<User>,
    }

    #[async_trait]
    impl UserRepository for MockUserRepository {
        async fn get_user_by_id(&self, user_id: i32) -> Result<User, UserError> {
            self.user.clone().ok_or(UserError::UserNotFound(user_id))
        }
        async fn update_user(&self, _user: User) -> Result<User, UserError> {
            self.update_user_result
                .clone()
                .ok_or(UserError::UnexpectedError("Cannot update user".to_string()))
        }
        async fn get_username(&self, username: &str) -> Result<String, UserError> {
            if self.username_exists {
                Ok(username.to_string())
            } else {
                Err(UserError::UserNameNotFound(username.to_string()))
            }
        }
        async fn update_username(&self, _user_id: i32, _username: &str) -> Result<User, UserError> {
            self.update_username_result
                .clone()
                .ok_or(UserError::UnexpectedError(
                    "Cannot update username".to_string(),
                ))
        }
    }

    fn sample_user() -> User {
        let now = DateTime::from_timestamp(0, 0).unwrap().naive_utc();

        User {
            id: 1,
            full_name: Some("Test User".to_string()),
            user_name: "testuser".to_string(),
            bio: Some("bio".to_string()),
            external_id: "extid".to_string(),
            avatar: Some("avatar.png".to_string()),
            created_at: now,
            updated_at: now,

            deleted_at: None,
            last_seen_at: None,
        }
    }

    #[tokio::test]
    async fn test_get_user_by_id_found() {
        let repo = MockUserRepository {
            user: Some(sample_user()),
            username_exists: true,
            update_user_result: None,
            update_username_result: None,
        };
        let service = UserServiceImpl::new(repo);
        let user = service.get_user_by_id(1).await.unwrap();
        assert_eq!(user.id, 1);
    }

    #[tokio::test]
    async fn test_get_user_by_id_not_found() {
        let repo = MockUserRepository {
            user: None,
            username_exists: false,
            update_user_result: None,
            update_username_result: None,
        };
        let service = UserServiceImpl::new(repo);
        let result = service.get_user_by_id(1).await;
        assert!(matches!(result, Err(UserError::UserNotFound(1))));
    }

    #[tokio::test]
    async fn test_update_user_success() {
        let user = sample_user();
        let updated_user = User {
            full_name: Some("Updated Name".to_string()),
            ..user.clone()
        };
        let repo = MockUserRepository {
            user: Some(user.clone()),
            username_exists: true,
            update_user_result: Some(updated_user.clone()),
            update_username_result: None,
        };
        let service = UserServiceImpl::new(repo);
        let dto = UpdateUserDto {
            full_name: "Updated Name".to_string(),
            avatar: Some("new_avatar.png".to_string()),
            bio: Some("new bio".to_string()),
        };
        let result = service.update_user(1, dto).await.unwrap();
        assert_eq!(result.full_name, Some("Updated Name".to_string()));
    }

    #[tokio::test]
    async fn test_update_user_not_found() {
        let repo = MockUserRepository {
            user: None,
            username_exists: true,
            update_user_result: None,
            update_username_result: None,
        };
        let service = UserServiceImpl::new(repo);
        let dto = UpdateUserDto {
            full_name: "Updated Name".to_string(),
            avatar: Some("new_avatar.png".to_string()),
            bio: Some("new bio".to_string()),
        };
        let result = service.update_user(1, dto).await;
        assert!(matches!(result, Err(UserError::UserNotFound(1))));
    }

    #[tokio::test]
    async fn test_check_username_exists_true() {
        let repo = MockUserRepository {
            user: None,
            username_exists: true,
            update_user_result: None,
            update_username_result: None,
        };
        let service = UserServiceImpl::new(repo);
        assert!(service.check_username_exists("testuser").await);
    }

    #[tokio::test]
    async fn test_check_username_exists_false() {
        let repo = MockUserRepository {
            user: None,
            username_exists: false,
            update_user_result: None,
            update_username_result: None,
        };
        let service = UserServiceImpl::new(repo);
        assert!(!service.check_username_exists("testuser").await);
    }

    #[tokio::test]
    async fn test_update_username_success() {
        let updated_user = User {
            user_name: "newname".to_string(),
            ..sample_user()
        };
        let repo = MockUserRepository {
            user: None,
            username_exists: false,
            update_user_result: None,
            update_username_result: Some(updated_user.clone()),
        };
        let service = UserServiceImpl::new(repo);
        let result = service.update_username(1, "newname").await.unwrap();
        assert_eq!(result.user_name, "newname");
    }

    #[tokio::test]
    async fn test_update_username_fail() {
        let repo = MockUserRepository {
            user: None,
            username_exists: false,
            update_user_result: None,
            update_username_result: None,
        };
        let service = UserServiceImpl::new(repo);
        let result = service.update_username(1, "newname").await;
        assert!(result.is_err());
    }
}
