use salvo::async_trait;

use diesel::{
    ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl, SelectableHelper,
    dsl::{delete, insert_into},
    r2d2::{ConnectionManager, Pool, PooledConnection},
};

use crate::core::{
    database::schema::ccus,
    entities::models::{Ccu, NewCcu},
    types::errors::{ccu_error::CcuError, general::GeneralError},
};

#[async_trait]
pub trait CcuRepository {
    async fn create_ccu(&self, ccu: NewCcu<'_>) -> Result<Ccu, CcuError>;

    async fn get_ccu_by_id(&self, ccu_id: i32) -> Result<Ccu, CcuError>;

    async fn delete_ccu_by_id(&self, ccu_id: i32) -> Result<(), CcuError>;
}

#[derive(Debug, Clone)]
pub struct CcuRepositoryImpl {
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl CcuRepositoryImpl {
    pub fn new(pool: Pool<ConnectionManager<PgConnection>>) -> Self {
        Self { pool }
    }

    fn get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, GeneralError> {
        self.pool.get().map_err(|_| GeneralError::DbConnectionError)
    }
}

#[async_trait]
impl CcuRepository for CcuRepositoryImpl {
    async fn create_ccu(&self, ccu: NewCcu<'_>) -> Result<Ccu, CcuError> {
        let mut conn = self.get_conn()?;

        let ccu = insert_into(ccus::table)
            .values(&ccu)
            .returning(Ccu::as_select())
            .get_result(&mut conn)
            .map_err(|_| CcuError::FailedToCreateCcu)?;

        Ok(ccu)
    }

    async fn get_ccu_by_id(&self, ccu_id: i32) -> Result<Ccu, CcuError> {
        let mut conn = self.get_conn()?;

        let ccu = ccus::table
            .filter(ccus::id.eq(ccu_id))
            .first::<Ccu>(&mut conn)
            .map_err(|_| CcuError::NotFoundCcu(ccu_id))?;

        Ok(ccu)
    }

    async fn delete_ccu_by_id(&self, ccu_id: i32) -> Result<(), CcuError> {
        let mut conn = self.get_conn()?;

        delete(ccus::table)
            .filter(ccus::id.eq(ccu_id))
            .execute(&mut conn)
            .map_err(|_| CcuError::FailedToDeleteCcu(ccu_id))?;

        Ok(())
    }
}
