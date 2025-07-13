use async_channel::Sender;
use tonic::transport::Server;
use tracing::info;
use waterbus_proto::dispatcher_service_server::DispatcherServiceServer;

use crate::{
    application::dispatcher_grpc_service::DispatcherGrpcService, domain::DispatcherCallback,
};

pub struct GrpcServer {}

impl GrpcServer {
    pub fn start(port: u16, sender: Sender<DispatcherCallback>) {
        info!("GrpcServer is running on port: {}", port);

        tokio::spawn(async move {
            match Self::start_server(port, sender).await {
                Ok(_) => info!("GrpcServer stopped successfully"),
                Err(e) => info!("GrpcServer stopped with an error: {:?}", e),
            }
        });
    }

    async fn start_server(port: u16, sender: Sender<DispatcherCallback>) -> anyhow::Result<()> {
        let addr = format!("0.0.0.0:{port}").parse().unwrap();

        let dispatcher_grpc_service = DispatcherGrpcService::new(sender);

        let shutdown_signal = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C signal handler");
        };
        Server::builder()
            .add_service(DispatcherServiceServer::new(dispatcher_grpc_service))
            .serve_with_shutdown(addr, shutdown_signal)
            .await?;

        Ok(())
    }
}
