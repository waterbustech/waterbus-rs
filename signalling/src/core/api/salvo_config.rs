use diesel::{
    PgConnection,
    r2d2::{ConnectionManager, Pool},
};
use reqwest::Method;
use rust_embed::RustEmbed;
use salvo::{
    catcher::Catcher,
    cors::{Any, Cors},
    oapi::{
        Contact, Info, License, SecurityRequirement, SecurityScheme,
        security::{Http, HttpAuthScheme},
    },
    prelude::*,
    rate_limiter::{BasicQuota, FixedGuard, MokaStore, RateLimiter, RemoteIpIssuer},
    serve_static::static_embed,
};
use typesense_client::TypesenseClient;

use crate::{
    core::{
        database::db::establish_connection, env::env_config::EnvConfig,
        socket::socket::get_socket_router, types::app_channel::AppEvent,
        utils::jwt_utils::JwtUtils,
    },
    features::{
        auth::{repository::AuthRepositoryImpl, router::get_auth_router, service::AuthServiceImpl},
        ccu::repository::CcuRepositoryImpl,
        chat::{repository::ChatRepositoryImpl, router::get_chat_router, service::ChatServiceImpl},
        meeting::{
            repository::MeetingRepositoryImpl, router::get_meeting_router,
            service::MeetingServiceImpl,
        },
        search::SearchService,
        sfu::service::SfuServiceImpl,
        user::{repository::UserRepositoryImpl, router::get_user_router, service::UserServiceImpl},
    },
};

#[derive(RustEmbed)]
#[folder = "../hls"]
struct HlsAssets;

#[derive(RustEmbed)]
#[folder = "../public"]
struct PublicAssets;

#[endpoint(tags("system"))]
async fn health_check(res: &mut Response) {
    res.render("[v3] Waterbus Service written in Rust");
}

#[handler]
async fn set_services(depot: &mut Depot) {
    let pool = depot.obtain::<DbConnection>().unwrap();
    let search_service = depot.obtain::<SearchService>().unwrap();

    let auth_repository = AuthRepositoryImpl::new(pool.clone().0);
    let user_repository = UserRepositoryImpl::new(pool.clone().0);
    let chat_repository = ChatRepositoryImpl::new(pool.clone().0);
    let meeting_repository = MeetingRepositoryImpl::new(pool.clone().0);

    let auth_service = AuthServiceImpl::new(auth_repository.clone());
    let chat_service = ChatServiceImpl::new(
        chat_repository.clone(),
        meeting_repository.clone(),
        user_repository.clone(),
    );

    let user_service = UserServiceImpl::new(user_repository.clone(), search_service.clone());
    let meeting_service =
        MeetingServiceImpl::new(meeting_repository.clone(), user_repository.clone());

    depot.inject(auth_service);
    depot.inject(user_service);
    depot.inject(chat_service);
    depot.inject(meeting_service);
}

pub async fn get_salvo_service(env: &EnvConfig) -> Service {
    let pool = establish_connection(env.clone());

    let db_pooled_connection = DbConnection(pool.clone());
    let jwt_utils = JwtUtils::new(env.clone());

    let typesense_client =
        TypesenseClient::new(env.typesense.uri.clone(), env.typesense.api_key.clone());
    let search_service = SearchService::new(typesense_client, pool.clone());
    search_service.init().await;

    let limiter = RateLimiter::new(
        FixedGuard::new(),
        MokaStore::new(),
        RemoteIpIssuer,
        BasicQuota::per_second(200),
    );

    let health_router = Router::new().path("/health-check").get(health_check);
    let auth_router = get_auth_router(jwt_utils.clone());
    let user_router = get_user_router(jwt_utils.clone());
    let chat_router = get_chat_router(jwt_utils.clone());
    let meeting_router = get_meeting_router(jwt_utils.clone());

    let (message_sender, message_receiver) = async_channel::unbounded::<AppEvent>();

    let meeting_repository = MeetingRepositoryImpl::new(pool.clone());
    let ccu_repository = CcuRepositoryImpl::new(pool.clone());
    let sfu_service = SfuServiceImpl::new(ccu_repository, meeting_repository);
    let socket_router = get_socket_router(&env, jwt_utils.clone(), sfu_service, message_receiver)
        .await
        .expect("Failed to config socket.io");

    let cors = Cors::new()
        .allow_origin(Any)
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
        .hoop(affix_state::inject(db_pooled_connection))
        .hoop(affix_state::inject(jwt_utils))
        .hoop(affix_state::inject(env.clone()))
        .hoop(affix_state::inject(search_service))
        .hoop(affix_state::inject(message_sender))
        .hoop(CatchPanic::new())
        .hoop(CachingHeaders::new())
        .hoop(Compression::new().min_length(1024))
        .hoop(limiter)
        .hoop(set_services)
        .push(auth_router)
        .push(chat_router)
        .push(user_router)
        .push(meeting_router)
        .push(health_router);

    let static_hls_router =
        Router::with_path("{*path}").get(static_embed::<HlsAssets>().fallback("index.html"));
    let static_router = Router::with_path("html/{*path}")
        .get(static_embed::<PublicAssets>().fallback("index.html"));

    let router = Router::new()
        .push(router)
        .push(socket_router)
        .push(static_router)
        .push(static_hls_router);

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

    let router = Router::new()
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("docs"))
        .push(router);

    Service::new(router)
        .hoop(cors)
        .catcher(Catcher::default().hoop(handle404))
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
