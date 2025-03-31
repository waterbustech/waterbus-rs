use diesel::{
    PgConnection,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use salvo::async_trait;

use crate::core::{
    entities::models::User,
    errors::{auth_error::AuthError, general::GeneralError},
};

#[async_trait]
trait AuthRepository: Send + Sync {
    async fn create_user(&self) -> Result<User, AuthError>;

    async fn get_user_by_id(&self, id: i32) -> Result<User, AuthError>;

    async fn get_user_by_user_name(&self, username: String) -> Result<User, AuthError>;

    async fn update_user(&self, user: User) -> Result<User, AuthError>;
}

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
    async fn create_user(&self) -> Result<User, AuthError> {
        let conn = self.get_conn()?;
        // Diesel operations here
        todo!()
    }

    async fn get_user_by_id(&self, id: i32) -> Result<User, AuthError> {
        let conn = self.get_conn()?;
        // Diesel operations here
        todo!()
    }

    async fn get_user_by_user_name(&self, username: String) -> Result<User, AuthError> {
        let conn = self.get_conn()?;
        // Diesel operations here
        todo!()
    }

    async fn update_user(&self, user: User) -> Result<User, AuthError> {
        let conn = self.get_conn()?;
        // Diesel operations here
        todo!()
    }
}
