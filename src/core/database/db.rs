use diesel::pg::PgConnection;
use diesel::r2d2::{self, ConnectionManager, PooledConnection};

use crate::core::env::env_config::EnvConfig;

pub type DbPool = r2d2::Pool<ConnectionManager<PgConnection>>;
pub type DbPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

pub fn establish_connection(env: EnvConfig) -> DbPool {
    let database_url = env.db_uri.0;

    let manager = ConnectionManager::<PgConnection>::new(database_url);
    r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create pool")
}
