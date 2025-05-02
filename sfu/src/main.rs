use sfu::infrastructure::{config::app_env::AppEnv, etcd::EtcdNode, grpc::grpc::GrpcServer};
use tracing::warn;
use webrtc_manager::models::WebRTCManagerConfigs;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt().init();

    let app_env = AppEnv::new();

    let webrtc_configs = WebRTCManagerConfigs {
        public_ip: app_env.public_ip,
        port_min: app_env.udp_port_range.port_min,
        port_max: app_env.udp_port_range.port_max,
    };

    let etcd_node =
        EtcdNode::register(app_env.etcd_addr, app_env.node_id, app_env.node_ip, 10).await?;

    GrpcServer::start(app_env.grpc_port.sfu_port, webrtc_configs);

    tokio::signal::ctrl_c().await?;

    etcd_node.deregister().await;

    warn!("Pod is shutting down...");

    Ok(())
}
