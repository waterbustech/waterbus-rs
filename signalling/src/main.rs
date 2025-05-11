use salvo::{
    conn::rustls::{Keycert, RustlsConfig},
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

    // Load TLS certificate and key
    let cert = include_bytes!("../../certificates/cert.pem").to_vec();
    let key = include_bytes!("../../certificates/key.pem").to_vec();
    let config = RustlsConfig::new(Keycert::new().cert(cert.as_slice()).key(key.as_slice()));

    let listener = TcpListener::new(http_addr.clone());
    let acceptor = QuinnListener::new(config.build_quinn_config().unwrap(), http_addr)
        .join(listener)
        .bind()
        .await;

    let server = Server::new(acceptor);
    let handle = server.handle();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl_c");
        tracing::info!("Signal received, shutting down gracefully...");
        handle.stop_graceful(None);
    });

    server.serve(get_salvo_service(&env).await).await;

    Ok(())
}
