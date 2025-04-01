use std::sync::Arc;

use salvo::{
    conn::rustls::{Keycert, RustlsConfig},
    oapi::{
        security::{Http, HttpAuthScheme}, SecurityScheme
    },
    prelude::*,
};
use waterbus_rs::core::{
    api::salvo_config::{DbConnection, get_api_router},
    database::db::establish_connection,
    env::env_config::EnvConfig,
    socket::socket::get_socket_router,
    utils::jwt_utils::JwtUtils,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let env = EnvConfig::new();
    let http2_addr = format!("127.0.0.1:{}", env.app_port.http2_port);
    let http3_addr = format!("127.0.0.1:{}", env.app_port.http3_port);

    let pool = establish_connection(env.clone());

    let db_pooled_connection = DbConnection(pool);
    let jwt_utils = JwtUtils::new(env.clone());

    let socket_router = get_socket_router(&env, jwt_utils.clone())
        .await
        .expect("Failed to config socket.io");
    let api_router = get_api_router(jwt_utils.clone()).await;

    let router = Router::new();

    let router = router.push(api_router).push(socket_router);

    let security_scheme = SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer));
    let doc = OpenApi::new("[v3] Waterbus Service API", "3.0.0")
        .add_security_scheme("BearerAuth", security_scheme)
        .merge_router(&router);

    let router = router
        .hoop(Logger::new())
        .hoop(affix_state::inject(db_pooled_connection))
        .hoop(affix_state::inject(jwt_utils))
        .hoop(affix_state::inject(env))
        .hoop(CatchPanic::new())
        .hoop(CachingHeaders::new())
        .hoop(Compression::new().min_length(1024))
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("docs"));

    // Load TLS certificate and key
    let cert = include_bytes!("../certificates/cert.pem").to_vec();
    let key = include_bytes!("../certificates/key.pem").to_vec();
    let config = RustlsConfig::new(Keycert::new().cert(cert.as_slice()).key(key.as_slice()));

    // HTTP/2 Listener
    let http2_listener = TcpListener::new(http2_addr).bind().await;

    // HTTP/3 Listener
    let http3_listener = QuinnListener::new(config, http3_addr).bind().await;

    // Run both servers concurrently
    let router = Arc::new(router);
    let router1 = Arc::clone(&router);
    let router2 = Arc::clone(&router);
    tokio::join!(
        async {
            Server::new(http2_listener).serve(router1).await;
        },
        async {
            Server::new(http3_listener).serve(router2).await;
        }
    );
}
