use std::sync::Arc;

use parking_lot::RwLock;
use rtc_manager::{
    models::{
        callbacks::{IceCandidateHandler, JoinedHandler, RenegotiationHandler},
        connection_type::ConnectionType,
        rtc_dto::{IceCandidate, JoinRoomParameters, RtcManagerConfig, SubscribeParameters},
        streaming_protocol::StreamingProtocol,
    },
    rtc_manager::RtcManager,
};
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use waterbus_proto::{
    AddPublisherCandidateRequest, AddSubscriberCandidateRequest, JoinRoomRequest, JoinRoomResponse,
    LeaveRoomRequest, LeaveRoomResponse, MigratePublisherRequest, MigratePublisherResponse,
    NewUserJoinedRequest, PublisherCandidateRequest, PublisherRenegotiationRequest,
    PublisherRenegotiationResponse, SetCameraType, SetEnabledRequest, SetScreenSharingRequest,
    SetSubscriberSdpRequest, StatusResponse, SubscribeHlsLiveStreamRequest,
    SubscribeHlsLiveStreamResponse, SubscribeRequest, SubscribeResponse,
    SubscriberCandidateRequest, SubscriberRenegotiateRequest, sfu_service_server::SfuService,
};

use super::dispacher_grpc_client::DispatcherGrpcClient;

pub struct SfuGrpcService {
    rtc_manager: Arc<RwLock<RtcManager>>,
    dispatcher_grpc_client: Arc<Mutex<DispatcherGrpcClient>>,
    node_id: String,
}

impl SfuGrpcService {
    pub fn new(
        config: RtcManagerConfig,
        dispatcher_grpc_client: Arc<Mutex<DispatcherGrpcClient>>,
        node_id: String,
    ) -> Self {
        let rtc_manager = Arc::new(RwLock::new(RtcManager::new(config)));

        Self {
            rtc_manager,
            dispatcher_grpc_client,
            node_id,
        }
    }
}

#[tonic::async_trait]
impl SfuService for SfuGrpcService {
    async fn join_room(
        &self,
        req: Request<JoinRoomRequest>,
    ) -> Result<Response<JoinRoomResponse>, Status> {
        let req = req.into_inner();

        let ice_handler = GrpcPublisherIceHandler {
            dispatcher: Arc::clone(&self.dispatcher_grpc_client),
            client_id: req.client_id.clone(),
        };

        let joined_handler = GrpcJoinedHandler {
            dispatcher: Arc::clone(&self.dispatcher_grpc_client),
            participant_id: req.participant_id.clone(),
            room_id: req.room_id.clone(),
            client_id: req.client_id.clone(),
            node_id: self.node_id.clone(),
        };

        let rtc_manager = self.rtc_manager.clone();
        let response = rtc_manager.write().join_room(JoinRoomParameters {
            client_id: req.client_id,
            participant_id: req.participant_id,
            room_id: req.room_id,
            sdp: req.sdp,
            is_video_enabled: req.is_video_enabled,
            is_audio_enabled: req.is_audio_enabled,
            is_e2ee_enabled: req.is_e2ee_enabled,
            total_tracks: req.total_tracks as u8,
            connection_type: ConnectionType::from(req.connection_type as u8),
            joined_handler: joined_handler.clone(),
            ice_handler: ice_handler.clone(),
            streaming_protocol: StreamingProtocol::from(req.streaming_protocol as u8),
            is_ipv6_supported: req.is_ipv6_supported,
        });

        match response {
            Ok(response) => match response {
                Some(response) => {
                    let join_room_response = JoinRoomResponse {
                        sdp: response.sdp,
                        is_recording: response.is_recording,
                    };
                    Ok(Response::new(join_room_response))
                }
                None => {
                    let join_room_response = JoinRoomResponse {
                        sdp: "".to_string(),
                        is_recording: false,
                    };
                    Ok(Response::new(join_room_response))
                }
            },
            Err(err) => Err(Status::internal(format!("Failed to join room: {err}"))),
        }
    }

    async fn subscribe(
        &self,
        req: Request<SubscribeRequest>,
    ) -> Result<Response<SubscribeResponse>, Status> {
        let req = req.into_inner();

        let ice_handler = GrpcSubscriberIceHandler {
            dispatcher: Arc::clone(&self.dispatcher_grpc_client),
            client_id: req.client_id.clone(),
            target_id: req.target_id.clone(),
        };

        let renegotiation_handler = GrpcRenegotiationHandler {
            dispatcher: Arc::clone(&self.dispatcher_grpc_client),
            client_id: req.client_id.clone(),
            target_id: req.target_id.clone(),
        };

        let rtc_manager = self.rtc_manager.clone();

        let response = rtc_manager.read().subscribe(SubscribeParameters {
            client_id: req.client_id,
            target_id: req.target_id,
            participant_id: req.participant_id,
            room_id: req.room_id,
            renegotiation_handler: renegotiation_handler.clone(),
            ice_handler: ice_handler.clone(),
            is_ipv6_supported: req.is_ipv6_supported,
        });

        match response {
            Ok(response) => {
                let subscribe_response = SubscribeResponse {
                    offer: response.offer,
                    camera_type: response.camera_type as u32,
                    video_enabled: response.video_enabled,
                    audio_enabled: response.audio_enabled,
                    is_screen_sharing: response.is_screen_sharing,
                    is_hand_raising: response.is_hand_raising,
                    is_e2ee_enabled: response.is_e2ee_enabled,
                    video_codec: response.video_codec,
                    screen_track_id: Some(response.screen_track_id),
                };
                Ok(Response::new(subscribe_response))
            }
            Err(err) => Err(Status::internal(format!("Failed to join room: {err}"))),
        }
    }

    async fn subscribe_hls_live_stream(
        &self,
        req: Request<SubscribeHlsLiveStreamRequest>,
    ) -> Result<Response<SubscribeHlsLiveStreamResponse>, Status> {
        let req = req.into_inner();

        let rtc_manager = self.rtc_manager.clone();
        let client_id = req.client_id.clone();
        let target_id = req.target_id.clone();
        let participant_id = req.participant_id.clone();
        let room_id = req.room_id.clone();

        let response = rtc_manager.read().subscribe_hls_live_stream(
            &client_id,
            &target_id,
            &participant_id,
            &room_id,
        );

        match response {
            Ok(response) => Ok(Response::new(SubscribeHlsLiveStreamResponse {
                hls_urls: vec![response.playlist_url],
            })),
            Err(err) => Err(Status::internal(format!(
                "Failed to subscribe hls live stream: {err}"
            ))),
        }
    }

    async fn set_subscriber_sdp(
        &self,
        req: Request<SetSubscriberSdpRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.rtc_manager.read();

        let response = writer.set_subscriber_sdp(&req.client_id, &req.target_id, req.sdp);

        match response {
            Ok(()) => {
                return Ok(Response::new(StatusResponse { is_success: true }));
            }
            Err(err) => {
                return Err(Status::internal(format!(
                    "Failed to set subscriber sdp: {err}"
                )));
            }
        }
    }

    async fn publisher_renegotiation(
        &self,
        req: Request<PublisherRenegotiationRequest>,
    ) -> Result<Response<PublisherRenegotiationResponse>, Status> {
        let req = req.into_inner();

        let response = tokio::task::spawn_blocking({
            let rtc_manager = self.rtc_manager.clone();
            let client_id = req.client_id.clone();
            let sdp = req.sdp.clone();

            move || {
                let writer = rtc_manager.read();

                writer.publisher_renegotiation(&client_id, sdp)
            }
        })
        .await
        .map_err(|e| Status::internal(format!("Task join error: {e}")))?;

        match response {
            Ok(sdp) => Ok(Response::new(PublisherRenegotiationResponse { sdp })),
            Err(err) => Err(Status::internal(format!(
                "Failed to handle publisher renegotiate: {err}"
            ))),
        }
    }

    async fn migrate_publisher_connection(
        &self,
        req: Request<MigratePublisherRequest>,
    ) -> Result<Response<MigratePublisherResponse>, Status> {
        let req = req.into_inner();

        let rtc_manager = self.rtc_manager.clone();
        let client_id = req.client_id.clone();
        let sdp = req.sdp.clone();
        let connection_type = ConnectionType::from(req.connection_type as u8);

        let response = tokio::task::spawn_blocking(move || {
            let writer = rtc_manager.read();

            writer.migrate_connection(&client_id, sdp, connection_type)
        })
        .await
        .map_err(|e| Status::internal(format!("Task join error: {e}")))?;

        match response {
            Ok(sdp) => Ok(Response::new(MigratePublisherResponse { sdp: Some(sdp) })),
            Err(err) => Err(Status::internal(format!(
                "Failed to handle publisher renegotiate: {err}"
            ))),
        }
    }

    async fn add_publisher_candidate(
        &self,
        req: Request<AddPublisherCandidateRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.rtc_manager.read();

        if let Some(candidate) = req.candidate {
            let response = writer.add_publisher_candidate(
                &req.client_id,
                IceCandidate {
                    candidate: candidate.candidate,
                    sdp_mid: candidate.sdp_mid,
                    sdp_m_line_index: candidate.sdp_m_line_index.map(|v| v as u16),
                },
            );

            match response {
                Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
                Err(err) => Err(Status::internal(format!(
                    "Failed to handle publisher renegotiate: {err}"
                ))),
            }
        } else {
            return Err(Status::invalid_argument("Missing ICE candidate"));
        }
    }

    async fn add_subscriber_candidate(
        &self,
        req: Request<AddSubscriberCandidateRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.rtc_manager.read();

        if let Some(candidate) = req.candidate {
            let response = writer.add_subscriber_candidate(
                &req.client_id,
                &req.target_id,
                IceCandidate {
                    candidate: candidate.candidate,
                    sdp_mid: candidate.sdp_mid,
                    sdp_m_line_index: candidate.sdp_m_line_index.map(|v| v as u16),
                },
            );

            match response {
                Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
                Err(err) => Err(Status::internal(format!(
                    "Failed to handle subscriber candidate: {err}"
                ))),
            }
        } else {
            return Err(Status::invalid_argument("Missing ICE candidate"));
        }
    }

    async fn leave_room(
        &self,
        req: Request<LeaveRoomRequest>,
    ) -> Result<Response<LeaveRoomResponse>, Status> {
        let req = req.into_inner();

        let writer = self.rtc_manager.read();

        let response = writer.leave_room(&req.client_id);

        match response {
            Ok(client) => Ok(Response::new(LeaveRoomResponse {
                participant_id: client.participant_id,
                room_id: client.room_id,
            })),
            Err(err) => Err(Status::internal(format!("Failed to leave room: {err}"))),
        }
    }

    async fn set_video_enabled(
        &self,
        req: Request<SetEnabledRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.rtc_manager.read();

        let response = writer.set_video_enabled(&req.client_id, req.is_enabled);

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set video enabled: {err}"
            ))),
        }
    }

    async fn set_audio_enabled(
        &self,
        req: Request<SetEnabledRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.rtc_manager.read();

        let response = writer.set_audio_enabled(&req.client_id, req.is_enabled);

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set audio enabled: {err}"
            ))),
        }
    }

    async fn set_hand_raising(
        &self,
        req: Request<SetEnabledRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.rtc_manager.write();

        let response = writer.set_hand_raising(&req.client_id, req.is_enabled);

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set hand raising: {err}"
            ))),
        }
    }

    async fn set_screen_sharing(
        &self,
        req: Request<SetScreenSharingRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.rtc_manager.write();

        let response =
            writer.set_screen_sharing(&req.client_id, req.is_enabled, req.screen_track_id);

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set screen sharing: {err}"
            ))),
        }
    }

    async fn set_camera_type(
        &self,
        req: Request<SetCameraType>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.rtc_manager.write();

        let response = writer.set_camera_type(&req.client_id, req.camera_type as u8);

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set camera type: {err}"
            ))),
        }
    }
}

#[derive(Clone)]
pub struct GrpcPublisherIceHandler {
    pub dispatcher: Arc<Mutex<DispatcherGrpcClient>>,
    pub client_id: String,
}

impl IceCandidateHandler for GrpcPublisherIceHandler {
    fn handle_candidate(&self, candidate: IceCandidate) {
        let dispatcher = Arc::clone(&self.dispatcher);
        let client_id = self.client_id.clone();

        tokio::spawn(async move {
            let dispatcher = dispatcher.lock().await;

            let _ = dispatcher
                .on_publisher_candidate(PublisherCandidateRequest {
                    client_id,
                    candidate: Some(waterbus_proto::common::IceCandidate {
                        candidate: candidate.candidate,
                        sdp_mid: candidate.sdp_mid,
                        sdp_m_line_index: candidate.sdp_m_line_index.map(|val| val as u32),
                    }),
                })
                .await;
        });
    }
}

#[derive(Clone)]
pub struct GrpcSubscriberIceHandler {
    pub dispatcher: Arc<Mutex<DispatcherGrpcClient>>,
    pub client_id: String,
    pub target_id: String,
}

impl IceCandidateHandler for GrpcSubscriberIceHandler {
    fn handle_candidate(&self, candidate: IceCandidate) {
        let dispatcher = Arc::clone(&self.dispatcher);
        let client_id = self.client_id.clone();
        let target_id = self.target_id.clone();
        tokio::spawn(async move {
            let dispatcher = dispatcher.lock().await;

            let _ = dispatcher
                .on_subscriber_candidate(SubscriberCandidateRequest {
                    client_id,
                    target_id,
                    candidate: Some(waterbus_proto::common::IceCandidate {
                        candidate: candidate.candidate,
                        sdp_mid: candidate.sdp_mid,
                        sdp_m_line_index: candidate.sdp_m_line_index.map(|val| val as u32),
                    }),
                })
                .await;
        });
    }
}

#[derive(Clone)]
pub struct GrpcJoinedHandler {
    pub dispatcher: Arc<Mutex<DispatcherGrpcClient>>,
    pub participant_id: String,
    pub room_id: String,
    pub client_id: String,
    pub node_id: String,
}

impl JoinedHandler for GrpcJoinedHandler {
    fn handle_joined(&self, is_migrate: bool) {
        let dispatcher = Arc::clone(&self.dispatcher);
        let participant_id = self.participant_id.clone();
        let room_id = self.room_id.clone();
        let client_id = self.client_id.clone();
        let node_id = self.node_id.clone();

        tokio::spawn(async move {
            let dispatcher = dispatcher.lock().await;

            let _ = dispatcher
                .new_user_joined(NewUserJoinedRequest {
                    participant_id,
                    room_id,
                    client_id,
                    node_id,
                    is_migrate,
                })
                .await;
        });
    }
}

#[derive(Clone)]
pub struct GrpcRenegotiationHandler {
    pub dispatcher: Arc<Mutex<DispatcherGrpcClient>>,
    pub client_id: String,
    pub target_id: String,
}

impl RenegotiationHandler for GrpcRenegotiationHandler {
    fn handle_renegotiation(&self, sdp: String) {
        let dispatcher = Arc::clone(&self.dispatcher);
        let client_id = self.client_id.clone();
        let target_id = self.target_id.clone();

        tokio::spawn(async move {
            let dispatcher = dispatcher.lock().await;
            let _ = dispatcher
                .subscriber_renegotiate(SubscriberRenegotiateRequest {
                    sdp,
                    client_id,
                    target_id,
                })
                .await;
        });
    }
}
