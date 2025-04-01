use diesel::{
    PgConnection,
    r2d2::{ConnectionManager, Pool},
};
use salvo::{
    prelude::*,
    rate_limiter::{BasicQuota, FixedGuard, MokaStore, RateLimiter, RemoteIpIssuer},
};

use crate::features::auth::router::get_auth_router;

#[endpoint]
async fn health_check(res: &mut Response) {
    res.render("[v3] Waterbus Service written in Rust");
}

pub async fn get_api_router() -> Router {
    let limiter = RateLimiter::new(
        FixedGuard::new(),
        MokaStore::new(),
        RemoteIpIssuer,
        BasicQuota::per_second(200),
    );

    let max_size = max_size(1024 * 1024 * 10);

    let health_router = Router::new().path("/health-check").get(health_check);
    let auth_router = get_auth_router();

    let router = Router::new()
        .hoop(limiter)
        .hoop(max_size)
        .push(auth_router)
        .push(health_router);

    router
}

#[derive(Debug, Clone)]
pub struct DbConnection(pub Pool<ConnectionManager<PgConnection>>);
