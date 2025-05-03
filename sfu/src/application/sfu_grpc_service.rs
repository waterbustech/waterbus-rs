use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tonic::{Request, Response, Status};
use waterbus_proto::{
    AddPublisherCandidateRequest, AddSubscriberCandidateRequest, JoinRoomRequest, JoinRoomResponse,
    LeaveRoomRequest, LeaveRoomResponse, NewUserJoinedRequest, PublisherCandidateRequest,
    PublisherRenegotiationRequest, PublisherRenegotiationResponse, SetCameraType,
    SetEnabledRequest, SetScreenSharingRequest, SetSubscriberSdpRequest, StatusResponse,
    SubscribeRequest, SubscribeResponse, SubscriberCandidateRequest, SubscriberRenegotiateRequest,
    sfu_service_server::SfuService,
};
use webrtc_manager::{
    models::{
        IceCandidate, IceCandidateCallback, JoinedCallback, RenegotiationCallback,
        WebRTCManagerConfigs,
    },
    webrtc_manager::{JoinRoomReq, WebRTCManager},
};

use super::dispacher_grpc_client::DispatcherGrpcClient;

#[derive(Debug)]
pub struct SfuGrpcService {
    webrtc_manager: Arc<RwLock<WebRTCManager>>,
    dispatcher_grpc_client: Arc<Mutex<DispatcherGrpcClient>>,
}

impl SfuGrpcService {
    pub fn new(
        configs: WebRTCManagerConfigs,
        dispatcher_grpc_client: Arc<Mutex<DispatcherGrpcClient>>,
    ) -> Self {
        let webrtc_manager = Arc::new(RwLock::new(WebRTCManager::new(configs)));

        Self {
            webrtc_manager,
            dispatcher_grpc_client,
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

        let writer = self.webrtc_manager.write().await;

        let dispatcher = Arc::clone(&self.dispatcher_grpc_client);
        let client_id = req.client_id.clone();
        let ice_candidate_callback: IceCandidateCallback =
            Arc::new(move |candidate: IceCandidate| {
                let dispatcher = Arc::clone(&dispatcher);
                let client_id = client_id.clone();

                Box::pin(async move {
                    let mut dispatcher = dispatcher.lock().await;

                    let _ = dispatcher
                        .on_publisher_candidate(PublisherCandidateRequest {
                            client_id: client_id,
                            candidate: Some(waterbus_proto::common::IceCandidate {
                                candidate: candidate.candidate,
                                sdp_mid: candidate.sdp_mid,
                                sdp_m_line_index: candidate.sdp_m_line_index.map(|val| val as u32),
                            }),
                        })
                        .await;
                })
            });

        let dispatcher = Arc::clone(&self.dispatcher_grpc_client);
        let participant_id = req.participant_id.clone();
        let room_id = req.room_id.clone();
        let client_id = req.client_id.clone();
        let joined_callback: JoinedCallback = Arc::new(move || {
            let dispatcher = Arc::clone(&dispatcher);
            let participant_id = participant_id.clone();
            let room_id = room_id.clone();
            let client_id = client_id.clone();

            Box::pin(async move {
                let mut dispatcher = dispatcher.lock().await;

                let _ = dispatcher
                    .new_user_joined(NewUserJoinedRequest {
                        participant_id,
                        room_id,
                        client_id,
                    })
                    .await;
            })
        });

        let response = writer
            .join_room(JoinRoomReq {
                client_id: req.client_id,
                participant_id: req.participant_id,
                room_id: req.room_id,
                sdp: req.sdp,
                is_video_enabled: req.is_video_enabled,
                is_audio_enabled: req.is_audio_enabled,
                is_e2ee_enabled: req.is_e2ee_enabled,
                total_tracks: req.total_tracks as u8,
                callback: joined_callback,
                ice_candidate_callback: ice_candidate_callback,
            })
            .await;

        match response {
            Ok(response) => {
                let join_room_response = JoinRoomResponse {
                    sdp: response.sdp,
                    is_recording: response.is_recording,
                };
                Ok(Response::new(join_room_response))
            }
            Err(err) => Err(Status::internal(format!("Failed to join room: {}", err))),
        }
    }

    async fn subscribe(
        &self,
        req: Request<SubscribeRequest>,
    ) -> Result<Response<SubscribeResponse>, Status> {
        let req = req.into_inner();

        let writer = self.webrtc_manager.write().await;

        let dispatcher = Arc::clone(&self.dispatcher_grpc_client);
        let client_id = req.client_id.clone();
        let target_id = req.target_id.clone();
        let renegotiation_callback: RenegotiationCallback = Arc::new(move |sdp| {
            let dispatcher = Arc::clone(&dispatcher);
            let client_id = client_id.clone();
            let target_id = target_id.clone();

            Box::pin(async move {
                let mut dispatcher = dispatcher.lock().await;

                let _ = dispatcher
                    .subsriber_renegotiate(SubscriberRenegotiateRequest {
                        sdp,
                        client_id,
                        target_id,
                    })
                    .await;
            })
        });

        let dispatcher = Arc::clone(&self.dispatcher_grpc_client);
        let client_id = req.client_id.clone();
        let target_id = req.target_id.clone();
        let ice_candidate_callback: IceCandidateCallback = Arc::new(move |candidate| {
            let dispatcher = Arc::clone(&dispatcher);
            let client_id = client_id.clone();
            let target_id = target_id.clone();

            Box::pin(async move {
                let mut dispatcher = dispatcher.lock().await;

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
            })
        });

        let response = writer
            .subscribe(
                &req.client_id,
                &req.target_id,
                renegotiation_callback,
                ice_candidate_callback,
            )
            .await;

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
                    screen_track_id: response.screen_track_id,
                };
                Ok(Response::new(subscribe_response))
            }
            Err(err) => Err(Status::internal(format!("Failed to join room: {}", err))),
        }
    }

    async fn set_subscriber_sdp(
        &self,
        req: Request<SetSubscriberSdpRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.webrtc_manager.write().await;

        let response = writer
            .set_subscriber_desc(&req.client_id, &req.target_id, &req.sdp)
            .await;

        match response {
            Ok(()) => {
                return Ok(Response::new(StatusResponse { is_success: true }));
            }
            Err(err) => {
                return Err(Status::internal(format!(
                    "Failed to set subscriber sdp: {}",
                    err
                )));
            }
        }
    }

    async fn publisher_renegotiation(
        &self,
        req: Request<PublisherRenegotiationRequest>,
    ) -> Result<Response<PublisherRenegotiationResponse>, Status> {
        let req = req.into_inner();

        let writer = self.webrtc_manager.write().await;

        let response = writer
            .handle_publisher_renegotiation(&req.client_id, &req.sdp)
            .await;

        match response {
            Ok(sdp) => Ok(Response::new(PublisherRenegotiationResponse { sdp })),
            Err(err) => Err(Status::internal(format!(
                "Failed to handle publisher renegotiate: {}",
                err
            ))),
        }
    }

    async fn add_publisher_candidate(
        &self,
        req: Request<AddPublisherCandidateRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.webrtc_manager.write().await;

        if let Some(candidate) = req.candidate {
            let response = writer
                .add_publisher_candidate(
                    &req.client_id,
                    IceCandidate {
                        candidate: candidate.candidate,
                        sdp_mid: candidate.sdp_mid,
                        sdp_m_line_index: candidate.sdp_m_line_index.map(|v| v as u16),
                    },
                )
                .await;

            match response {
                Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
                Err(err) => Err(Status::internal(format!(
                    "Failed to handle publisher renegotiate: {}",
                    err
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

        let writer = self.webrtc_manager.write().await;

        if let Some(candidate) = req.candidate {
            let response = writer
                .add_subscriber_candidate(
                    &req.client_id,
                    &req.target_id,
                    IceCandidate {
                        candidate: candidate.candidate,
                        sdp_mid: candidate.sdp_mid,
                        sdp_m_line_index: candidate.sdp_m_line_index.map(|v| v as u16),
                    },
                )
                .await;

            match response {
                Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
                Err(err) => Err(Status::internal(format!(
                    "Failed to handle subscriber candidate: {}",
                    err
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

        let writer = self.webrtc_manager.write().await;

        let response = writer.leave_room(&req.client_id).await;

        match response {
            Ok(client) => Ok(Response::new(LeaveRoomResponse {
                participant_id: client.participant_id,
                room_id: client.room_id,
            })),
            Err(err) => Err(Status::internal(format!("Failed to leave room: {}", err))),
        }
    }

    async fn set_video_enabled(
        &self,
        req: Request<SetEnabledRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.webrtc_manager.write().await;

        let response = writer
            .set_video_enabled(&req.client_id, req.is_enabled)
            .await;

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set video enabled: {}",
                err
            ))),
        }
    }

    async fn set_audio_enabled(
        &self,
        req: Request<SetEnabledRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.webrtc_manager.write().await;

        let response = writer
            .set_audio_enabled(&req.client_id, req.is_enabled)
            .await;

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set audio enabled: {}",
                err
            ))),
        }
    }

    async fn set_hand_raising(
        &self,
        req: Request<SetEnabledRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.webrtc_manager.write().await;

        let response = writer
            .set_hand_raising(&req.client_id, req.is_enabled)
            .await;

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set hand raising: {}",
                err
            ))),
        }
    }

    async fn set_screen_sharing(
        &self,
        req: Request<SetScreenSharingRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.webrtc_manager.write().await;

        let response = writer
            .set_screen_sharing(&req.client_id, req.is_enabled, req.screen_track_id)
            .await;

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set screen sharing: {}",
                err
            ))),
        }
    }

    async fn set_camera_type(
        &self,
        req: Request<SetCameraType>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = req.into_inner();

        let writer = self.webrtc_manager.write().await;

        let response = writer
            .set_camera_type(&req.client_id, req.camera_type as u8)
            .await;

        match response {
            Ok(()) => Ok(Response::new(StatusResponse { is_success: true })),
            Err(err) => Err(Status::internal(format!(
                "Failed to set camera type: {}",
                err
            ))),
        }
    }
}
