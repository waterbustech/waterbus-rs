use salvo::async_trait;

use crate::core::{
    dtos::user::update_user_dto::UpdateUserDto, entities::models::User,
    types::errors::user_error::UserError,
};

#[async_trait]
pub trait ChatService: Send + Sync {
    async fn get_user_by_id(&self, user_id: i32) -> Result<User, UserError>;
    async fn update_user(&self, data: UpdateUserDto) -> Result<User, UserError>;
    async fn search_user(&self, query: &str) -> Result<Vec<User>, UserError>;
    async fn check_username_exists(&self, username: &str) -> bool;
    async fn update_username(&self, username: &str) -> Result<Vec<User>, UserError>;
}
