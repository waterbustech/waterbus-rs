use std::sync::Arc;

use tokio::sync::RwLock;
use tonic::transport::Server;
use tracing::info;
use waterbus_proto::sfu_service_server::SfuServiceServer;
use webrtc_manager::models::WebRTCManagerConfigs;

use crate::application::{
    dispacher_grpc_client::DispatcherGrpcClient, sfu_grpc_service::SfuGrpcService,
};

pub struct GrpcServer {}

impl GrpcServer {
    pub fn start(port: u16, configs: WebRTCManagerConfigs) {
        info!("GrpcServer is running on port: {}", port);

        tokio::spawn(async move {
            match Self::start_server(port, configs).await {
                Ok(_) => info!("GrpcServer stopped successfully"),
                Err(e) => info!("AppServer<Grpc> stopped with an error: {:?}", e),
            }
        });
    }

    async fn start_server(port: u16, configs: WebRTCManagerConfigs) -> anyhow::Result<()> {
        let addr = format!("[::1]:{}", port).parse().unwrap();

        let dispatcher_grpc_client = DispatcherGrpcClient::new(port);

        let sfu_grpc_service =
            SfuGrpcService::new(configs, Arc::new(RwLock::new(dispatcher_grpc_client)));

        let shutdown_signal = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C signal handler");
        };
        Server::builder()
            .add_service(SfuServiceServer::new(sfu_grpc_service))
            .serve_with_shutdown(addr, shutdown_signal)
            .await?;

        Ok(())
    }
}
