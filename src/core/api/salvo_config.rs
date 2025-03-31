use diesel::{
    PgConnection,
    r2d2::{ConnectionManager, Pool},
};
use salvo::{
    prelude::*,
    rate_limiter::{BasicQuota, FixedGuard, MokaStore, RateLimiter, RemoteIpIssuer},
};

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

    let router = Router::new()
        .hoop(limiter)
        .hoop(max_size)
        .path("/health-check")
        .get(health_check);

    router
}

#[derive(Debug, Clone)]
pub struct DbConnection(pub Pool<ConnectionManager<PgConnection>>);
