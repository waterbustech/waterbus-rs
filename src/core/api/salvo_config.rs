use diesel::{
    PgConnection,
    r2d2::{ConnectionManager, Pool},
};
use salvo::{
    prelude::*,
    rate_limiter::{BasicQuota, FixedGuard, MokaStore, RateLimiter, RemoteIpIssuer},
};

use crate::{
    core::utils::jwt_utils::JwtUtils,
    features::{
        auth::router::get_auth_router, chat::router::get_chat_router,
        meeting::router::get_meeting_router, user::router::get_user_router,
    },
};

#[endpoint]
async fn health_check(res: &mut Response) {
    res.render("[v3] Waterbus Service written in Rust");
}

pub async fn get_api_router(jwt_utils: JwtUtils) -> Router {
    let limiter = RateLimiter::new(
        FixedGuard::new(),
        MokaStore::new(),
        RemoteIpIssuer,
        BasicQuota::per_second(200),
    );

    let max_size = max_size(1024 * 1024 * 10);

    let health_router = Router::new().path("/health-check").get(health_check);
    let auth_router = get_auth_router(jwt_utils.clone());
    let user_router = get_user_router(jwt_utils.clone());
    let chat_router = get_chat_router(jwt_utils.clone());
    let meeting_router = get_meeting_router(jwt_utils.clone());

    let router = Router::new()
        .hoop(limiter)
        .hoop(max_size)
        .push(auth_router)
        .push(chat_router)
        .push(user_router)
        .push(meeting_router)
        .push(health_router);

    router
}

#[derive(Debug, Clone)]
pub struct DbConnection(pub Pool<ConnectionManager<PgConnection>>);
