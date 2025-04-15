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

use waterbus_rs::core::{api::config::get_salvo_service, env::env_config::EnvConfig};

#[tokio::main]
async fn main() {
    let filter = EnvFilter::new("info")
        .add_directive("webrtc_srtp::session=info".parse().unwrap())
        .add_directive("webrtc_ice::agent::agent_internal=off".parse().unwrap());

    let filter_fn = FilterFn::new(|meta: &Metadata<'_>| {
        let is_webrtc_session = meta.target().contains("webrtc_srtp::session");
        let is_webrtc_ice = meta.target().contains("webrtc_ice::agent::agent_internal");
        !(is_webrtc_session || is_webrtc_ice)
    });

    registry()
        .with(filter)
        .with(fmt::layer().with_filter(filter_fn))
        .init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let env = EnvConfig::new();
    let http2_addr = format!("0.0.0.0:{}", env.app_port.http2_port);
    let http3_addr = format!("0.0.0.0:{}", env.app_port.http3_port);

    // Load TLS certificate and key
    let cert = include_bytes!("../certificates/cert.pem").to_vec();
    let key = include_bytes!("../certificates/key.pem").to_vec();
    let config = RustlsConfig::new(Keycert::new().cert(cert.as_slice()).key(key.as_slice()));

    // HTTP/2 Listener
    let http2_listener = TcpListener::new(http2_addr).bind().await;

    // HTTP/3 Listener
    let http3_listener = QuinnListener::new(config, http3_addr).bind().await;

    tokio::join!(
        async {
            Server::new(http2_listener)
                .serve(get_salvo_service(&env).await)
                .await;
        },
        async {
            Server::new(http3_listener)
                .serve(get_salvo_service(&env).await)
                .await;
        }
    );
}
