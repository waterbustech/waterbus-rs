use salvo::{
    conn::rustls::{Keycert, RustlsConfig},
    prelude::*,
};
use tracing::Metadata;
use tracing_subscriber::{
    filter::{EnvFilter, FilterFn},
    fmt,
    layer::{Layer, SubscriberExt},
    registry,
    util::SubscriberInitExt,
};

use signalling::core::{api::config::get_salvo_service, env::env_config::EnvConfig};

#[tokio::main]
async fn main() {
    let filter = EnvFilter::new("info")
        .add_directive("webrtc_srtp::session=info".parse().unwrap())
        .add_directive("webrtc_ice::agent::agent_internal=off".parse().unwrap())
        .add_directive(
            "webrtc::peer_connection::peer_connection_internal=off"
                .parse()
                .unwrap(),
        );

    let filter_fn = FilterFn::new(|meta: &Metadata<'_>| {
        let is_webrtc_session = meta.target().contains("webrtc_srtp::session");
        let is_webrtc_ice = meta.target().contains("webrtc_ice::agent::agent_internal");
        let is_webrtc_pc_internal = meta
            .target()
            .contains("webrtc::peer_connection::peer_connection_internal");

        !(is_webrtc_session || is_webrtc_ice || is_webrtc_pc_internal)
    });

    registry()
        .with(filter)
        .with(fmt::layer().with_filter(filter_fn))
        .init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let env = EnvConfig::new();
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

    Server::new(acceptor)
        .serve(get_salvo_service(&env).await)
        .await;
}
