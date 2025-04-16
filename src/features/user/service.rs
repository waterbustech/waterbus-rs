use salvo::async_trait;
use serde_json::Value;

use crate::{
    core::{
        dtos::{common::page_request_dto::PageRequestDto, user::update_user_dto::UpdateUserDto},
        entities::models::User,
        types::errors::user_error::UserError,
    },
    features::search::SearchService,
};

use super::repository::{UserRepository, UserRepositoryImpl};

#[async_trait]
pub trait UserService: Send + Sync {
    async fn get_user_by_id(&self, user_id: i32) -> Result<User, UserError>;
    async fn update_user(&self, user_id: i32, data: UpdateUserDto) -> Result<User, UserError>;
    async fn search_user(
        &self,
        query: &str,
        page_request_dto: PageRequestDto,
    ) -> Result<Value, UserError>;
    async fn check_username_exists(&self, username: &str) -> bool;
    async fn update_username(&self, user_id: i32, username: &str) -> Result<User, UserError>;
}

#[derive(Debug, Clone)]
pub struct UserServiceImpl {
    repository: UserRepositoryImpl,
    search_service: SearchService,
}

impl UserServiceImpl {
    pub fn new(repository: UserRepositoryImpl, search_service: SearchService) -> Self {
        Self {
            repository: repository,
            search_service: search_service,
        }
    }
}

#[async_trait]
impl UserService for UserServiceImpl {
    async fn get_user_by_id(&self, user_id: i32) -> Result<User, UserError> {
        let user = self.repository.get_user_by_id(user_id).await;

        user
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

                let updated_user = self.repository.update_user(updated_user).await;

                updated_user
            }
            Err(err) => Err(err),
        }
    }

    async fn search_user(
        &self,
        query: &str,
        page_request_dto: PageRequestDto,
    ) -> Result<Value, UserError> {
        self.search_service
            .search_users(
                query,
                Some(page_request_dto.page),
                Some(page_request_dto.per_page),
            )
            .await
            .map_err(|err| UserError::General(err))
    }

    async fn check_username_exists(&self, username: &str) -> bool {
        let user = self.repository.get_username(username).await;

        match user {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    async fn update_username(&self, user_id: i32, username: &str) -> Result<User, UserError> {
        let user = self.repository.update_username(user_id, username).await;

        user
    }
}
