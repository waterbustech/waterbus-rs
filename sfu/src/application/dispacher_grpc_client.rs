use tonic::{Request, Status, transport::Channel};
use tracing::warn;
use waterbus_proto::{
    NewUserJoinedRequest, PublisherCandidateRequest, SubscriberCandidateRequest,
    SubscriberRenegotiateRequest, dispatcher_service_client::DispatcherServiceClient,
};

#[derive(Debug, Clone, Default)]
pub struct DispatcherGrpcClient {
    host: String,
    port: u16,
}

impl DispatcherGrpcClient {
    pub fn new(host: String, port: u16) -> Self {
        Self { port, host }
    }

    async fn get_client(
        &self,
    ) -> Result<DispatcherServiceClient<Channel>, tonic::transport::Error> {
        let addr = format!("{}:{}", self.host, self.port);
        DispatcherServiceClient::connect(addr).await
    }

    pub async fn new_user_joined(&self, req: NewUserJoinedRequest) -> Result<(), Status> {
        let mut client = self
            .get_client()
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to dispatcher: {e}")))?;

        client
            .new_user_joined(Request::new(req))
            .await
            .map(|_| ())
            .map_err(|e| {
                warn!("Error sending new_user_joined: {:?}", e);
                e
            })
    }

    pub async fn subscriber_renegotiate(
        &self,
        req: SubscriberRenegotiateRequest,
    ) -> Result<(), Status> {
        let mut client = self
            .get_client()
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to dispatcher: {e}")))?;

        client
            .subscriber_renegotiate(Request::new(req))
            .await
            .map(|_| ())
            .map_err(|e| {
                warn!("Error sending subscriber_renegotiate: {:?}", e);
                e
            })
    }

    pub async fn on_publisher_candidate(
        &self,
        req: PublisherCandidateRequest,
    ) -> Result<(), Status> {
        let mut client = self
            .get_client()
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to dispatcher: {e}")))?;

        client
            .on_publisher_candidate(Request::new(req))
            .await
            .map(|_| ())
            .map_err(|e| {
                warn!("Error sending on_publisher_candidate: {:?}", e);
                e
            })
    }

    pub async fn on_subscriber_candidate(
        &self,
        req: SubscriberCandidateRequest,
    ) -> Result<(), Status> {
        let mut client = self
            .get_client()
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to dispatcher: {e}")))?;

        client
            .on_subscriber_candidate(Request::new(req))
            .await
            .map(|_| ())
            .map_err(|e| {
                warn!("Error sending on_subscriber_candidate: {:?}", e);
                e
            })
    }
}
