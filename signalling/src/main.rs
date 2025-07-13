use salvo::{
    conn::{
        Acceptor,
        rustls::{Keycert, RustlsConfig},
    },
    prelude::*,
};
use signalling::core::{api::salvo_config::get_salvo_service, env::app_env::AppEnv};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt().init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let env = AppEnv::new();
    let http_addr = format!("0.0.0.0:{}", env.app_port);

    if env.tls_enabled {
        run_tls_server(&env, http_addr).await;
    } else {
        run_plain_server(&env, &http_addr).await;
    }

    Ok(())
}

async fn run_tls_server(env: &AppEnv, http_addr: String) {
    // Load TLS cert/key
    let cert = include_bytes!("../../certificates/cert.pem").to_vec();
    let key = include_bytes!("../../certificates/key.pem").to_vec();
    let config = RustlsConfig::new(Keycert::new().cert(cert.as_slice()).key(key.as_slice()));

    let listener = TcpListener::new(http_addr.clone()).rustls(config.clone());
    let acceptor = QuinnListener::new(config.build_quinn_config().unwrap(), http_addr)
        .join(listener)
        .bind()
        .await;

    let server = Server::new(acceptor);
    run_server(server, env).await;
}

async fn run_plain_server(env: &AppEnv, http_addr: &str) {
    let acceptor = TcpListener::new(http_addr).bind().await;
    let server = Server::new(acceptor);
    run_server(server, env).await;
}

async fn run_server<A: Acceptor + Send>(server: Server<A>, env: &AppEnv) {
    let handle = server.handle();

    // Graceful shutdown handler
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl_c");
        tracing::info!("Signal received, shutting down gracefully...");
        handle.stop_graceful(None);
    });

    // Start the server
    server.serve(get_salvo_service(env).await).await;
}
