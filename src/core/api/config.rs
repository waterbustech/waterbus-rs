use diesel::{
    PgConnection,
    r2d2::{ConnectionManager, Pool},
};
use salvo::{
    prelude::*,
    rate_limiter::{BasicQuota, FixedGuard, MokaStore, RateLimiter, RemoteIpIssuer},
};

use crate::{
    core::{
        env::env_config::EnvConfig, socket::socket::get_socket_router, utils::jwt_utils::JwtUtils,
    },
    features::{
        auth::{repository::AuthRepositoryImpl, router::get_auth_router, service::AuthServiceImpl},
        chat::{repository::ChatRepositoryImpl, router::get_chat_router, service::ChatServiceImpl},
        meeting::{
            repository::MeetingRepositoryImpl, router::get_meeting_router,
            service::MeetingServiceImpl,
        },
        user::{repository::UserRepositoryImpl, router::get_user_router, service::UserServiceImpl},
    },
};

#[endpoint(tags("system"))]
async fn health_check(res: &mut Response) {
    res.render("[v3] Waterbus Service written in Rust");
}

#[handler]
async fn set_services(depot: &mut Depot) {
    let pool = depot.obtain::<DbConnection>().unwrap();

    let auth_repository = AuthRepositoryImpl::new(pool.clone().0);
    let user_repository = UserRepositoryImpl::new(pool.clone().0);
    let chat_repository = ChatRepositoryImpl::new(pool.clone().0);
    let meeting_repository: MeetingRepositoryImpl = MeetingRepositoryImpl::new(pool.clone().0);

    let auth_service = AuthServiceImpl::new(auth_repository.clone());
    let chat_service = ChatServiceImpl::new(
        chat_repository.clone(),
        meeting_repository.clone(),
        user_repository.clone(),
    );
    let user_service = UserServiceImpl::new(user_repository.clone());
    let meeting_service =
        MeetingServiceImpl::new(meeting_repository.clone(), user_repository.clone());

    depot.inject(auth_service);
    depot.inject(user_service);
    depot.inject(chat_service);
    depot.inject(meeting_service);
}

pub async fn get_api_router(env: &EnvConfig, jwt_utils: JwtUtils) -> Router {
    let limiter = RateLimiter::new(
        FixedGuard::new(),
        MokaStore::new(),
        RemoteIpIssuer,
        BasicQuota::per_second(200),
    );

    let max_size = max_size(1024 * 1024 * 10);

    let health_router = Router::new().path("/health-check").get(health_check);
    let socket_router = get_socket_router(&env, jwt_utils.clone())
        .await
        .expect("Failed to config socket.io");
    let auth_router = get_auth_router(jwt_utils.clone());
    let user_router = get_user_router(jwt_utils.clone());
    let chat_router = get_chat_router(jwt_utils.clone());
    let meeting_router = get_meeting_router(jwt_utils.clone());

    let router = Router::with_path("busapi/v3")
        .hoop(limiter)
        .hoop(max_size)
        .hoop(set_services)
        .push(auth_router)
        .push(chat_router)
        .push(user_router)
        .push(meeting_router)
        .push(socket_router)
        .push(health_router);

    router
}

#[derive(Debug, Clone)]
pub struct DbConnection(pub Pool<ConnectionManager<PgConnection>>);
