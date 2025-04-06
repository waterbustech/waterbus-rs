use diesel::{
    PgConnection,
    r2d2::{ConnectionManager, Pool},
};
use reqwest::Method;
use salvo::{
    catcher::Catcher,
    cors::Cors,
    oapi::{
        Contact, Info, License, SecurityRequirement, SecurityScheme,
        security::{Http, HttpAuthScheme},
    },
    prelude::*,
    rate_limiter::{BasicQuota, FixedGuard, MokaStore, RateLimiter, RemoteIpIssuer},
};

use crate::{
    core::{
        database::db::establish_connection, env::env_config::EnvConfig,
        socket::socket::get_socket_router, types::app_channel::AppEvent,
        utils::jwt_utils::JwtUtils,
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

pub async fn get_salvo_service(env: &EnvConfig) -> Service {
    let pool = establish_connection(env.clone());

    let db_pooled_connection = DbConnection(pool);
    let jwt_utils = JwtUtils::new(env.clone());

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

    let (app_channel_tx, app_channel_rx) = async_channel::unbounded::<AppEvent>();

    let socket_router = get_socket_router(&env, jwt_utils.clone(), app_channel_tx, app_channel_rx)
        .await
        .expect("Failed to config socket.io");

    let cors = Cors::new()
        .allow_origin("*") // Allow all origins
        .allow_methods(vec![
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::PUT,
            Method::OPTIONS,
        ])
        .allow_headers(vec!["Authorization", "Content-Type"])
        .into_handler();

    let router = Router::with_path("busapi/v3")
        .hoop(Logger::new())
        .hoop(cors)
        .hoop(affix_state::inject(db_pooled_connection))
        .hoop(affix_state::inject(jwt_utils))
        .hoop(affix_state::inject(env.clone()))
        .hoop(CatchPanic::new())
        .hoop(CachingHeaders::new())
        .hoop(Compression::new().min_length(1024))
        .hoop(limiter)
        .hoop(max_size)
        .hoop(set_services)
        .push(auth_router)
        .push(chat_router)
        .push(user_router)
        .push(meeting_router)
        .push(socket_router)
        .push(health_router);

    // Config
    let doc_info = Info::new("[v3] Waterbus Service API", "3.0.0")
    .description(
        "Open source video conferencing app built on latest WebRTC SDK. Android/iOS/MacOS/Windows/Linux/Web",
    )
    .license(License::new("Apache-2.0"))
    .contact(Contact::new().name("Kai").email("lambiengcode@gmail.com"));
    let http_auth_schema = Http::new(HttpAuthScheme::Bearer)
        .bearer_format("JWT")
        .description("jsonwebtoken");
    let security_scheme = SecurityScheme::Http(http_auth_schema);
    let security_requirement = SecurityRequirement::new("BearerAuth", ["*"]);
    let doc = OpenApi::new("[v3] Waterbus Service API", "3.0.0")
        .info(doc_info.clone())
        .add_security_scheme("BearerAuth", security_scheme)
        .security([security_requirement])
        .merge_router(&router);

    let router = router
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("docs"));

    Service::new(router).catcher(Catcher::default().hoop(handle404))
}

#[handler]
async fn handle404(res: &mut Response, ctrl: &mut FlowCtrl) {
    if StatusCode::NOT_FOUND == res.status_code.unwrap_or(StatusCode::NOT_FOUND) {
        res.render("[v3] Waterbus Not Found");
        ctrl.skip_rest();
    }
}

#[derive(Debug, Clone)]
pub struct DbConnection(pub Pool<ConnectionManager<PgConnection>>);
