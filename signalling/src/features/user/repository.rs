use diesel::{
    ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl, SelectableHelper,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    update,
};
use salvo::async_trait;

use crate::core::{
    database::schema::users,
    entities::models::User,
    types::errors::{general::GeneralError, user_error::UserError},
};

#[async_trait]
pub trait UserRepository {
    async fn get_user_by_id(&self, user_id: i32) -> Result<User, UserError>;
    async fn update_user(&self, user: User) -> Result<User, UserError>;
    async fn get_username(&self, username: &str) -> Result<String, UserError>;
    async fn update_username(&self, user_id: i32, username: &str) -> Result<User, UserError>;
}

#[derive(Debug, Clone)]
pub struct UserRepositoryImpl {
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl UserRepositoryImpl {
    pub fn new(pool: Pool<ConnectionManager<PgConnection>>) -> Self {
        Self { pool }
    }

    fn get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, GeneralError> {
        self.pool.get().map_err(|_| GeneralError::DbConnectionError)
    }
}

#[async_trait]
impl UserRepository for UserRepositoryImpl {
    async fn get_user_by_id(&self, user_id: i32) -> Result<User, UserError> {
        let mut conn = self.get_conn()?;

        let user = users::table
            .filter(users::id.eq(user_id))
            .first::<User>(&mut conn);

        match user {
            Ok(user) => Ok(user),
            Err(_) => Err(UserError::UserNotFound(user_id)),
        }
    }

    async fn update_user(&self, user: User) -> Result<User, UserError> {
        let mut conn = self.get_conn()?;

        let updated_user = update(users::table)
            .filter(users::id.eq(user.id))
            .set((
                users::full_name.eq(user.full_name),
                users::avatar.eq(user.avatar),
                users::bio.eq(user.bio),
            ))
            .returning(User::as_select())
            .get_result(&mut conn);

        match updated_user {
            Ok(user) => Ok(user),
            Err(_) => Err(UserError::UnexpectedError(
                "Cannot update username".to_string(),
            )),
        }
    }

    async fn get_username(&self, username: &str) -> Result<String, UserError> {
        let mut conn = self.get_conn()?;

        let user_name = users::table
            .filter(users::user_name.eq(username))
            .select(users::user_name)
            .first::<String>(&mut conn);

        match user_name {
            Ok(user_name) => Ok(user_name),
            Err(_) => Err(UserError::UserNameNotFound(username.to_string())),
        }
    }

    async fn update_username(&self, user_id: i32, username: &str) -> Result<User, UserError> {
        let mut conn = self.get_conn()?;

        let updated_user = update(users::table)
            .filter(users::id.eq(user_id))
            .set(users::user_name.eq(username))
            .returning(User::as_select())
            .get_result(&mut conn);

        match updated_user {
            Ok(user) => Ok(user),
            Err(_) => Err(UserError::UnexpectedError(
                "Cannot update username".to_string(),
            )),
        }
    }
}
