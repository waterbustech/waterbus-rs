use salvo::{oapi::extract::QueryParam, prelude::*};
use tracing::info;
use waterbus_rs::core::{
    api::salvo_config::{DbConnection, get_api_router},
    database::db::establish_connection,
    env::env_config::EnvConfig,
    socket::socket::get_socket_router,
    utils::jwt_utils::JwtUtils,
};

#[endpoint]
async fn hello(name: QueryParam<String, false>) -> String {
    format!("Hello, {}!", name.as_deref().unwrap_or("World"))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let env = EnvConfig::new();
    let port = env.app_port;
    let local_addr = format!("127.0.0.1:{}", port);

    info!(local_addr);

    let pool = establish_connection(env.clone());

    let db_pooled_connection = DbConnection(pool);
    let jwt_utils = JwtUtils::new(env.clone());

    let socket_router = get_socket_router()
        .await
        .expect("Failed to config socket.io");
    let api_router = get_api_router().await;

    let router = Router::new();

    let router = router.push(api_router).push(socket_router);
    let doc = OpenApi::new("[v3] Waterbus Service API", "3.0.0").merge_router(&router);

    let router = router
        .hoop(affix_state::inject(db_pooled_connection))
        .hoop(affix_state::inject(jwt_utils))
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("swagger-ui"));

    let acceptor = TcpListener::new(local_addr).bind().await;

    Server::new(acceptor).serve(router).await;
}
