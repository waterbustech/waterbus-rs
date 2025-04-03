use diesel::{
    ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl, SelectableHelper,
    dsl::insert_into,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use salvo::async_trait;

use crate::core::{
    database::schema::users,
    entities::models::{NewUser, User},
    types::errors::{auth_error::AuthError, general::GeneralError},
};

#[async_trait]
pub trait AuthRepository: Send + Sync {
    async fn create_user(&self, user: NewUser<'_>) -> Result<User, AuthError>;

    async fn get_user_by_id(&self, id: i32) -> Result<User, AuthError>;

    async fn get_user_by_auth_id(
        &self,
        google_id: Option<&str>,
        github_id: Option<&str>,
    ) -> Result<User, AuthError>;

    async fn get_user_by_user_name(&self, username: String) -> Result<User, AuthError>;
}

#[derive(Debug, Clone)]
pub struct AuthRepositoryImpl {
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl AuthRepositoryImpl {
    pub fn new(pool: Pool<ConnectionManager<PgConnection>>) -> Self {
        Self { pool }
    }

    fn get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, GeneralError> {
        self.pool.get().map_err(|_| GeneralError::DbConnectionError)
    }
}

#[async_trait]
impl AuthRepository for AuthRepositoryImpl {
    async fn create_user(&self, user: NewUser<'_>) -> Result<User, AuthError> {
        let mut conn = self.get_conn()?;

        let new_user = insert_into(users::table)
            .values(&user)
            .returning(User::as_select())
            .get_result(&mut conn);

        match new_user {
            Ok(user) => Ok(user),
            Err(_) => Err(AuthError::UnexpectedError(
                "Cannot insert user to DB".to_string(),
            )),
        }
    }

    async fn get_user_by_id(&self, id: i32) -> Result<User, AuthError> {
        let mut conn = self.get_conn()?;

        let user = users::table
            .filter(users::id.eq(id))
            .first::<User>(&mut conn);

        match user {
            Ok(user) => Ok(user),
            Err(_) => Err(AuthError::UserNotFound(id)),
        }
    }

    async fn get_user_by_auth_id(
        &self,
        google_id: Option<&str>,
        github_id: Option<&str>,
    ) -> Result<User, AuthError> {
        let mut conn = self.get_conn()?;

        let user = users::table
            .filter(users::google_id.eq(google_id))
            .or_filter(users::github_id.eq(github_id))
            .first::<User>(&mut conn);

        match user {
            Ok(user) => Ok(user),
            Err(_) => Err(AuthError::UserNotFound(0)),
        }
    }

    async fn get_user_by_user_name(&self, username: String) -> Result<User, AuthError> {
        let mut conn = self.get_conn()?;

        let user = users::table
            .filter(users::user_name.eq(username))
            .first::<User>(&mut conn);

        match user {
            Ok(user) => Ok(user),
            Err(_) => Err(AuthError::UserNotFound(0)),
        }
    }
}
