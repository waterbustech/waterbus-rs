use async_channel::Sender;
use tonic::{Request, Response, Status};
use waterbus_proto::dispatcher_service_server::DispatcherService;
use waterbus_proto::{
    DispatcherResponse, NewUserJoinedRequest, PublisherCandidateRequest,
    SubscriberCandidateRequest, SubscriberRenegotiateRequest,
};

use crate::domain::DispatcherCallback;

#[derive(Debug)]
pub struct DispatcherGrpcService {
    sender: Sender<DispatcherCallback>,
}

impl DispatcherGrpcService {
    pub fn new(sender: Sender<DispatcherCallback>) -> Self {
        Self { sender }
    }
}

#[tonic::async_trait]
impl DispatcherService for DispatcherGrpcService {
    async fn new_user_joined(
        &self,
        req: Request<NewUserJoinedRequest>,
    ) -> Result<Response<DispatcherResponse>, Status> {
        let req = req.into_inner();
        let _ = self
            .sender
            .send(DispatcherCallback::NewUserJoined(req))
            .await;

        Ok(Response::new(DispatcherResponse { is_success: true }))
    }

    async fn subscriber_renegotiate(
        &self,
        req: Request<SubscriberRenegotiateRequest>,
    ) -> Result<Response<DispatcherResponse>, Status> {
        let req = req.into_inner();
        let _ = self
            .sender
            .send(DispatcherCallback::SubscriberRenegotiate(req))
            .await;

        Ok(Response::new(DispatcherResponse { is_success: true }))
    }

    async fn on_publisher_candidate(
        &self,
        req: Request<PublisherCandidateRequest>,
    ) -> Result<Response<DispatcherResponse>, Status> {
        let req = req.into_inner();
        let _ = self
            .sender
            .send(DispatcherCallback::PublisherCandidate(req))
            .await;

        Ok(Response::new(DispatcherResponse { is_success: true }))
    }

    async fn on_subscriber_candidate(
        &self,
        req: Request<SubscriberCandidateRequest>,
    ) -> Result<Response<DispatcherResponse>, Status> {
        let req = req.into_inner();
        let _ = self
            .sender
            .send(DispatcherCallback::SubscriberCandidate(req))
            .await;

        Ok(Response::new(DispatcherResponse { is_success: true }))
    }
}
