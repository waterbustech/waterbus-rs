use diesel::pg::PgConnection;
use diesel::r2d2::{self, ConnectionManager, PooledConnection};
use tracing::{error, info};

use crate::core::env::app_env::AppEnv;

pub type DbPool = r2d2::Pool<ConnectionManager<PgConnection>>;
pub type DbPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

pub fn establish_connection(env: AppEnv) -> DbPool {
    let database_url = &env.db_uri.0;

    let manager = ConnectionManager::<PgConnection>::new(database_url);

    let pool = r2d2::Pool::builder().build(manager).unwrap_or_else(|e| {
        error!("Failed to create pool: {}", e);
        panic!("Database pool creation failed");
    });

    info!("Connected to database: {}", database_url);

    pool
}
