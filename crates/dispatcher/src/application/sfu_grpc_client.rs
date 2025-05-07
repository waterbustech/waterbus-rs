use tonic::{Request, Status, transport::Channel};
use waterbus_proto::{
    AddPublisherCandidateRequest, AddSubscriberCandidateRequest, JoinRoomRequest, JoinRoomResponse,
    LeaveRoomRequest, LeaveRoomResponse, PublisherRenegotiationRequest,
    PublisherRenegotiationResponse, SetCameraType, SetEnabledRequest, SetScreenSharingRequest,
    SetSubscriberSdpRequest, StatusResponse, SubscribeRequest, SubscribeResponse,
    sfu_service_client::SfuServiceClient,
};

#[derive(Debug, Clone, Default)]
pub struct SfuGrpcClient {}

impl SfuGrpcClient {
    async fn get_client(
        &self,
        server_address: String,
    ) -> Result<SfuServiceClient<Channel>, tonic::transport::Error> {
        // let addr = format!("http://[::1]:{}", 50051);
        SfuServiceClient::connect(server_address).await
    }

    pub async fn join_room(
        &self,
        server_address: String,
        request: JoinRoomRequest,
    ) -> Result<tonic::Response<JoinRoomResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client.join_room(Request::new(request)).await?;
        Ok(response)
    }

    pub async fn subscribe(
        &self,
        server_address: String,
        request: SubscribeRequest,
    ) -> Result<tonic::Response<SubscribeResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client.subscribe(Request::new(request)).await?;
        Ok(response)
    }

    pub async fn set_subscriber_sdp(
        &self,
        server_address: String,
        request: SetSubscriberSdpRequest,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client.set_subscriber_sdp(Request::new(request)).await?;
        Ok(response)
    }

    pub async fn publisher_renegotiation(
        &self,
        server_address: String,
        request: PublisherRenegotiationRequest,
    ) -> Result<tonic::Response<PublisherRenegotiationResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client
            .publisher_renegotiation(Request::new(request))
            .await?;
        Ok(response)
    }

    pub async fn add_publisher_candidate(
        &self,
        server_address: String,
        request: AddPublisherCandidateRequest,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client
            .add_publisher_candidate(Request::new(request))
            .await?;
        Ok(response)
    }

    pub async fn add_subscriber_candidate(
        &self,
        server_address: String,
        request: AddSubscriberCandidateRequest,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client
            .add_subscriber_candidate(Request::new(request))
            .await?;
        Ok(response)
    }

    pub async fn leave_room(
        &self,
        server_address: String,
        request: LeaveRoomRequest,
    ) -> Result<tonic::Response<LeaveRoomResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client.leave_room(Request::new(request)).await?;
        Ok(response)
    }

    pub async fn set_video_enabled(
        &self,
        server_address: String,
        request: SetEnabledRequest,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client.set_video_enabled(Request::new(request)).await?;
        Ok(response)
    }

    pub async fn set_audio_enabled(
        &self,
        server_address: String,
        request: SetEnabledRequest,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client.set_audio_enabled(Request::new(request)).await?;
        Ok(response)
    }

    pub async fn set_hand_raising(
        &self,
        server_address: String,
        request: SetEnabledRequest,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client.set_hand_raising(Request::new(request)).await?;
        Ok(response)
    }

    pub async fn set_screen_sharing(
        &self,
        server_address: String,
        request: SetScreenSharingRequest,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client.set_screen_sharing(Request::new(request)).await?;
        Ok(response)
    }

    pub async fn set_camera_type(
        &self,
        server_address: String,
        request: SetCameraType,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        let mut client = self
            .get_client(server_address)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to SFU: {}", e)))?;
        let response = client.set_camera_type(Request::new(request)).await?;
        Ok(response)
    }
}
