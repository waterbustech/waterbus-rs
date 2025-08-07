use sfu::infrastructure::{config::app_env::AppEnv, etcd::EtcdNode, grpc::GrpcServer};
use tracing::{Metadata, warn};
use tracing_subscriber::{
    EnvFilter, Layer, filter::FilterFn, fmt, layer::SubscriberExt, registry,
    util::SubscriberInitExt,
};
use rtc_manager::models::params::RtcManagerConfigs;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
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

    let app_env = AppEnv::new();

    let rtc_configs = RtcManagerConfigs {
        public_ip: app_env.public_ip,
        port_min: app_env.udp_port_range.port_min,
        port_max: app_env.udp_port_range.port_max,
    };

    let ttl = 10;

    let etcd_node = EtcdNode::register(
        app_env.etcd_addr,
        app_env.node_id.clone(),
        app_env.grpc_configs.sfu_host,
        app_env.group_id,
        ttl,
    )
    .await?;

    GrpcServer::start(
        app_env.grpc_configs.sfu_port,
        app_env.grpc_configs.dispatcher_host,
        app_env.grpc_configs.dispatcher_port,
        rtc_configs,
        app_env.node_id,
    );

    tokio::signal::ctrl_c().await?;

    etcd_node.deregister().await;

    warn!("Pod is shutting down...");

    Ok(())
}
