use salvo::prelude::*;
use tracing::info;
use waterbus_rs::core::{
    api::salvo_config::get_api_router, database::db::establish_connection,
    env::env_config::EnvConfig, socket::socket::get_socket_router,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let env = EnvConfig::new();
    let port = env.app_port;
    let local_addr = format!("127.0.0.1:{}", port);

    info!(local_addr);

    let pool = establish_connection(env.clone());
    let mut conn = pool.get().expect("Failed to get DB connection");

    let socket_router = get_socket_router()
        .await
        .expect("Failed to config socket.io");
    let api_router = get_api_router().await;

    let router = api_router.push(socket_router);
    let acceptor = TcpListener::new(local_addr).bind().await;

    Server::new(acceptor).serve(router).await;
}
