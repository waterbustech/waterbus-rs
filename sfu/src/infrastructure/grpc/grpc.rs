use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::transport::Server;
use tracing::info;
use waterbus_proto::sfu_service_server::SfuServiceServer;
use webrtc_manager::models::WebRTCManagerConfigs;

use crate::application::{
    dispacher_grpc_client::DispatcherGrpcClient, sfu_grpc_service::SfuGrpcService,
};

pub struct GrpcServer {}

impl GrpcServer {
    pub fn start(
        port: u16,
        dispatcher_host: String,
        dispatcher_port: u16,
        configs: WebRTCManagerConfigs,
        node_id: String,
    ) {
        info!("GrpcServer is running on port: {}", port);

        tokio::spawn(async move {
            match Self::start_server(port, dispatcher_host, dispatcher_port, configs, node_id).await
            {
                Ok(_) => info!("GrpcServer stopped successfully"),
                Err(e) => info!("GrpcServer stopped with an error: {:?}", e),
            }
        });
    }

    async fn start_server(
        port: u16,
        dispatcher_host: String,
        dispatcher_port: u16,
        configs: WebRTCManagerConfigs,
        node_id: String,
    ) -> anyhow::Result<()> {
        let addr = format!("0.0.0.0:{}", port).parse().unwrap();

        let dispatcher_grpc_client = Arc::new(Mutex::new(DispatcherGrpcClient::new(
            dispatcher_host,
            dispatcher_port,
        )));

        let sfu_grpc_service = SfuGrpcService::new(configs, dispatcher_grpc_client, node_id);

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
