use diesel::{
    PgConnection,
    r2d2::{ConnectionManager, Pool},
};
use salvo::prelude::*;

#[endpoint]
async fn health_check(res: &mut Response) {
    res.render("[v3] Waterbus Service written in Rust");
}

pub async fn get_api_router() -> Router {
    let router = Router::with_path("/health-check").get(health_check);

    router
}

#[derive(Debug, Clone)]
pub struct DbConnection(pub Pool<ConnectionManager<PgConnection>>);
