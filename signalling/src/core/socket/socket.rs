use std::sync::Arc;

use anyhow::anyhow;
use async_channel::{Receiver, Sender};
use salvo::prelude::*;
use socketioxide::{
    SocketIo,
    adapter::Adapter,
    extract::{Data, Extension, SocketRef, State},
    handler::ConnectHandler,
};
use socketioxide_redis::{RedisAdapter, RedisAdapterCtr, drivers::redis::redis_client as redis};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use webrtc_manager::{
    errors::WebRTCError,
    models::{
        IceCandidate, IceCandidateCallback, JoinedCallback, RenegotiationCallback, WClient,
        WebRTCManagerConfigs,
    },
    webrtc_manager::{JoinRoomReq, WebRTCManager},
};

use crate::{
    core::{
        dtos::socket::socket_dto::{
            AnswerSubscribeDto, CandidateDto, CleanWhiteBoardDto, JoinRoomDto, MsgDto,
            PublisherRenegotiationDto, SetCameraTypeDto, SetEnabledDto, SetHandRaisingDto,
            SetScreenSharingDto, StartWhiteBoardDto, SubscribeDto, SubscriberCandidateDto,
            UpdateWhiteBoardDto,
        },
        env::env_config::EnvConfig,
        types::{
            app_channel::{AppChannel, AppEvent},
            enums::socket_event::SocketEvent,
            res::socket_response::{
                CameraTypeResponse, EnabledResponse, HandleRaisingResponse,
                MeetingSubscribeResponse, ParticipantHasLeftResponse, ScreenSharingResponse,
                SubscriberRenegotiationResponse, SubsriberCandidateResponse,
            },
        },
        utils::jwt_utils::JwtUtils,
    },
    features::sfu::service::{SfuService, SfuServiceImpl},
};

#[derive(Clone)]
pub struct UserId(pub String);

#[endpoint(tags("socket.io"))]
async fn version() -> &'static str {
    "[v3] Waterbus Service written in Rust"
}

#[derive(Clone)]
struct RemoteUserCnt(redis::aio::MultiplexedConnection);
impl RemoteUserCnt {
    fn new(conn: redis::aio::MultiplexedConnection) -> Self {
        Self(conn)
    }
    async fn add_user(&self) -> Result<usize, redis::RedisError> {
        let mut conn = self.0.clone();
        let num_users: usize = redis::cmd("INCR")
            .arg("num_users")
            .query_async(&mut conn)
            .await?;
        Ok(num_users)
    }
    async fn remove_user(&self) -> Result<usize, redis::RedisError> {
        let mut conn = self.0.clone();
        let num_users: usize = redis::cmd("DECR")
            .arg("num_users")
            .query_async(&mut conn)
            .await?;
        Ok(num_users)
    }
}

pub async fn get_socket_router(
    env: &EnvConfig,
    jwt_utils: JwtUtils,
    sfu_service: SfuServiceImpl,
    async_channel_tx: Sender<AppEvent>,
    async_channel_rx: Receiver<AppEvent>,
) -> Result<Router, Box<dyn std::error::Error>> {
    let client = redis::Client::open(env.clone().redis_uri.0)?;
    let adapter = RedisAdapterCtr::new_with_redis(&client).await?;
    let conn = client.get_multiplexed_tokio_connection().await?;

    let app_channel = AppChannel {
        async_channel_rx,
        async_channel_tx,
    };

    let (layer, io) = SocketIo::builder()
        .with_state(RemoteUserCnt::new(conn))
        .with_state(jwt_utils.clone())
        .with_state(app_channel)
        .with_state(sfu_service.clone())
        .with_state(WebRTCManager::new(WebRTCManagerConfigs {
            public_ip: env.public_ip.clone(),
            port_min: env.udp_port_range.port_min,
            port_max: env.udp_port_range.port_max,
        }))
        .with_adapter::<RedisAdapter<_>>(adapter)
        .build_layer();

    let layer = ServiceBuilder::new()
        .layer(CorsLayer::permissive()) // Enable CORS policy
        .layer(layer);

    io.ns("/", on_connect.with(authenticate_middleware)).await?;

    let layer = layer.compat();
    let router = Router::new().hoop(layer).path("/socket.io").goal(version);

    Ok(router)
}

async fn authenticate_middleware<A: Adapter>(
    s: SocketRef<A>,
    State(user_cnt): State<RemoteUserCnt>,
    State(jwt_utils): State<JwtUtils>,
) -> Result<(), anyhow::Error> {
    let auth_header = s
        .req_parts()
        .headers
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or(anyhow::anyhow!("Missing Authorization header"))?;

    let token = auth_header.trim_start_matches("Bearer ");

    match jwt_utils.decode_token(token) {
        Ok(claims) => {
            let user_id = claims.id;
            let _ = user_cnt.add_user().await.unwrap_or(0);
            s.extensions.insert(UserId(user_id.clone()));
            Ok(())
        }
        Err(err) => {
            warn!("decode token failed: {:?}", err);
            Err(anyhow!("Invalid token"))
        }
    }
}

async fn on_connect<A: Adapter>(
    socket: SocketRef<A>,
    sfu_service: State<SfuServiceImpl>,
    user_id: Extension<UserId>,
) {
    info!("user {:?} connected", user_id.0.0);

    let socket_id = socket.id.to_string();
    let user_id = user_id.0.0;

    let user_id_parsed = match user_id.parse::<i32>() {
        Ok(id) => id,
        Err(e) => {
            warn!("Failed to parse user_id as i32: {:?}", e);
            return;
        }
    };

    _handle_on_connection(user_id_parsed, &socket_id, sfu_service.0).await;

    socket.on("message", handle_msg);

    socket.on(SocketEvent::ReconnectCSS.to_str(), on_reconnect);
    socket.on(SocketEvent::PublishCSS.to_str(), handle_join_room);
    socket.on(SocketEvent::SubscribeCSS.to_str(), handle_subscribe);
    socket.on(
        SocketEvent::AnswerSubscriberCSS.to_str(),
        handle_answer_subscribe,
    );
    socket.on(
        SocketEvent::PublisherRenegotiationCSS.to_str(),
        handle_publisher_renegotiation,
    );
    socket.on(
        SocketEvent::PublisherCandidateCSS.to_str(),
        handle_publisher_candidate,
    );
    socket.on(
        SocketEvent::SubscriberCandidateCSS.to_str(),
        handle_subscriber_candidate,
    );
    socket.on(
        SocketEvent::SetE2eeEnabledCSS.to_str(),
        handle_set_e2ee_enabled,
    );
    socket.on(
        SocketEvent::SetCameraTypeCSS.to_str(),
        handle_set_camera_type,
    );
    socket.on(
        SocketEvent::SetVideoEnabledCSS.to_str(),
        handle_set_video_enabled,
    );
    socket.on(
        SocketEvent::SetAudioEnabledCSS.to_str(),
        handle_set_audio_enabled,
    );
    socket.on(
        SocketEvent::SetScreenSharingCSS.to_str(),
        handle_set_screen_sharing,
    );
    socket.on(
        SocketEvent::HandRaisingCSS.to_str(),
        handle_set_hand_raising,
    );
    socket.on(
        SocketEvent::StartWhiteBoardCSS.to_str(),
        handle_start_white_board,
    );
    socket.on(
        SocketEvent::UpdateWhiteBoardCSS.to_str(),
        handle_update_white_board,
    );
    socket.on(
        SocketEvent::CleanWhiteBoardCSS.to_str(),
        handle_clean_white_board,
    );
    socket.on(
        SocketEvent::SetSubscribeSubtitleCSS.to_str(),
        handle_set_subscribe_subtitle,
    );
    socket.on(SocketEvent::LeaveRoomCSS.to_str(), handle_leave_room);

    socket.on_disconnect(on_disconnect);
}

async fn on_disconnect<A: Adapter>(
    socket: SocketRef<A>,
    user_cnt: State<RemoteUserCnt>,
    webrtc_manager: State<WebRTCManager>,
    sfu_service: State<SfuServiceImpl>,
) {
    let _ = user_cnt.remove_user().await.unwrap_or(0);

    let _ = _handle_leave_room(socket, webrtc_manager.0.clone(), sfu_service.0, true).await;
}

async fn handle_msg<A: Adapter>(socket: SocketRef<A>, Data(data): Data<MsgDto>) {
    socket.join("lambiengcode");
    socket.emit("message", &data).ok();
    socket
        .broadcast()
        .to("lambiengcode")
        .emit("message", &data)
        .await
        .ok();
}

async fn on_reconnect<A: Adapter>(_: SocketRef<A>) {}

async fn handle_join_room<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<JoinRoomDto>,
    webrtc_manager: State<WebRTCManager>,
    sfu_service: State<SfuServiceImpl>,
) {
    let client_id = socket.id.to_string();
    let participant_id = &data.participant_id;
    let room_id = data.room_id.clone();

    webrtc_manager.clone().add_client(
        &client_id,
        WClient {
            participant_id: participant_id.clone(),
            room_id: room_id.clone(),
        },
    );

    let participant_id_parsed = match participant_id.parse::<i32>() {
        Ok(id) => id,
        Err(e) => {
            warn!("Failed to parse participant_id as i32: {:?}", e);
            return;
        }
    };

    let participant = sfu_service
        .update_participant(participant_id_parsed, &client_id)
        .await;

    if let Ok(participant) = participant {
        let socket_clone = socket.clone();
        let room_id_for_callback = room_id.clone();
        let socket_clone_for_ice = socket_clone.clone();
        let socket_clone_for_joined = socket_clone.clone();

        let ice_candidate_callback: IceCandidateCallback =
            Arc::new(move |candidate: IceCandidate| {
                let socket_clone_for_ice = socket_clone_for_ice.clone();
                Box::pin(async move {
                    let _ = socket_clone_for_ice
                        .emit(SocketEvent::PublisherCandidateSSC.to_str(), &candidate)
                        .ok();
                })
            });

        let joined_callback: JoinedCallback = Arc::new(move || {
            println!("Joined callback triggered!");
            let socket_clone_for_joined = socket_clone_for_joined.clone();
            let room_id_for_callback = room_id_for_callback.clone();
            let paricipant_clone = participant.clone();

            Box::pin(async move {
                let _ = socket_clone_for_joined
                    .broadcast()
                    .to(room_id_for_callback.clone())
                    .emit(SocketEvent::NewParticipantSSC.to_str(), &paricipant_clone)
                    .await
                    .ok();
            })
        });

        let req = JoinRoomReq {
            sdp: data.sdp,
            is_audio_enabled: data.is_audio_enabled,
            is_video_enabled: data.is_video_enabled,
            is_e2ee_enabled: data.is_e2ee_enabled,
            total_tracks: data.total_tracks,
            callback: joined_callback,
            ice_candidate_callback,
        };

        match webrtc_manager.clone().join_room(&client_id, req).await {
            Ok(res) => {
                socket.join(room_id.clone());

                let _ = socket.emit(SocketEvent::PublishSSC.to_str(), &res).ok();
            }
            Err(err) => {
                warn!("Err: {:?}", err)
            }
        }
    }
}

async fn handle_subscribe<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SubscribeDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let target_id = data.target_id;

    let socket_clone_for_renegotiation = socket.clone();
    let target_id_clone = target_id.clone();

    let renegotiation_callback: RenegotiationCallback = Arc::new(move |sdp| {
        info!("send sdp subscriber due to renegotiation needed trigger!");
        let _ = socket_clone_for_renegotiation
            .emit(
                SocketEvent::SubscriberRenegotiationSSC.to_str(),
                &SubscriberRenegotiationResponse {
                    target_id: target_id_clone.clone(),
                    sdp: sdp,
                },
            )
            .ok();
    });

    let socket_clone = socket.clone();
    let target_id_clone = target_id.clone();

    let ice_candidate_callback: IceCandidateCallback = Arc::new(move |candidate| {
        let socket_clone_for_ice = socket_clone.clone();
        let target_id_clone_for_ice = target_id_clone.clone();

        Box::pin(async move {
            let _ = socket_clone_for_ice
                .emit(
                    SocketEvent::SubscriberCandidateSSC.to_str(),
                    &SubsriberCandidateResponse {
                        candidate: candidate,
                        target_id: target_id_clone_for_ice.clone(),
                    },
                )
                .ok();
        })
    });

    let res = webrtc_manager
        .subscribe(
            &client_id,
            &target_id,
            renegotiation_callback,
            ice_candidate_callback,
        )
        .await;

    if let Ok(res) = res {
        let _ = socket
            .emit(
                SocketEvent::AnswerSubscriberSSC.to_str(),
                &MeetingSubscribeResponse {
                    subscribe_response: res,
                    target_id: target_id,
                },
            )
            .ok();
    }
}

async fn handle_answer_subscribe<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<AnswerSubscribeDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let target_id = data.target_id;
    let sdp = data.sdp;

    let _ = webrtc_manager
        .set_subscriber_desc(&client_id, &target_id, &sdp)
        .await;
}

async fn handle_publisher_renegotiation<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<PublisherRenegotiationDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let sdp = data.sdp;

    let sdp = webrtc_manager
        .handle_publisher_renegotiation(&client_id, &sdp)
        .await;

    match sdp {
        Ok(sdp) => {
            let _ = socket
                .emit(
                    SocketEvent::PublisherRenegotiationSSC.to_str(),
                    &PublisherRenegotiationDto { sdp: sdp },
                )
                .ok();
        }
        Err(_) => {}
    }
}

async fn handle_publisher_candidate<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<CandidateDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let candidate = data;

    let candidate = IceCandidate {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_m_line_index: candidate.sdp_m_line_index,
    };

    let _ = webrtc_manager
        .add_publisher_candidate(&client_id, candidate)
        .await;
}

async fn handle_subscriber_candidate<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SubscriberCandidateDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let candidate = data.candidate;
    let target_id = data.target_id;

    let candidate = IceCandidate {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_m_line_index: candidate.sdp_m_line_index,
    };

    let _ = webrtc_manager
        .add_subscriber_candidate(&client_id, &target_id, candidate)
        .await;
}

async fn handle_set_e2ee_enabled<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetEnabledDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let is_enabled = data.is_enabled;

    let client = {
        match webrtc_manager.get_client_by_id(&client_id) {
            Ok(client) => client,
            Err(_) => return,
        }
    };
    let client = client.clone();

    let room_id = client.room_id;
    let participant_id = client.participant_id;

    let _ = webrtc_manager
        .set_e2ee_enabled(&client_id, is_enabled)
        .await;

    let _ = socket
        .broadcast()
        .to(room_id)
        .emit(
            SocketEvent::SetE2eeEnabledSSC.to_str(),
            &EnabledResponse {
                participant_id: participant_id,
                is_enabled,
            },
        )
        .await
        .ok();
}

async fn handle_set_camera_type<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetCameraTypeDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let camera_type = data.type_;

    let client = {
        match webrtc_manager.get_client_by_id(&client_id) {
            Ok(client) => client,
            Err(_) => return,
        }
    };
    let client = client.clone();

    let room_id = client.room_id;
    let participant_id = client.participant_id;

    let camera_type_parsed: Result<u8, _> = camera_type.try_into();

    match camera_type_parsed {
        Ok(parsed_type) => {
            let _ = webrtc_manager
                .set_camera_type(&client_id, parsed_type)
                .await;

            let _ = socket
                .broadcast()
                .to(room_id)
                .emit(
                    SocketEvent::SetCameraTypeSSC.to_str(),
                    &CameraTypeResponse {
                        participant_id: participant_id,
                        type_: camera_type,
                    },
                )
                .await
                .ok();
        }
        Err(e) => {
            warn!("Failed to convert camera type to u8: {:?}", e);
        }
    }
}

async fn handle_set_video_enabled<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetEnabledDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let is_enabled = data.is_enabled;

    let client = {
        match webrtc_manager.get_client_by_id(&client_id) {
            Ok(client) => client,
            Err(_) => return,
        }
    };
    let client = client.clone();

    let room_id = client.room_id;
    let participant_id = client.participant_id;

    let _ = webrtc_manager
        .set_video_enabled(&client_id, is_enabled)
        .await;

    let _ = socket
        .broadcast()
        .to(room_id)
        .emit(
            SocketEvent::SetVideoEnabledSSC.to_str(),
            &EnabledResponse {
                participant_id: participant_id,
                is_enabled,
            },
        )
        .await
        .ok();
}

async fn handle_set_audio_enabled<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetEnabledDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let is_enabled = data.is_enabled;

    let client = {
        match webrtc_manager.get_client_by_id(&client_id) {
            Ok(client) => client,
            Err(_) => return,
        }
    };
    let client = client.clone();

    let room_id = client.room_id;
    let participant_id = client.participant_id;

    let _ = webrtc_manager
        .set_audio_enabled(&client_id, is_enabled)
        .await;

    let _ = socket
        .broadcast()
        .to(room_id)
        .emit(
            SocketEvent::SetAudioEnabledSSC.to_str(),
            &EnabledResponse {
                participant_id: participant_id,
                is_enabled,
            },
        )
        .await
        .ok();
}

async fn handle_set_screen_sharing<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetScreenSharingDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let is_enabled = data.is_sharing;
    let screen_track_id = data.screen_track_id;

    let client = {
        match webrtc_manager.get_client_by_id(&client_id) {
            Ok(client) => client,
            Err(_) => return,
        }
    };
    let client = client.clone();

    let room_id = client.room_id;
    let participant_id = client.participant_id;

    let _ = webrtc_manager
        .set_screen_sharing(&client_id, is_enabled, screen_track_id.clone())
        .await;

    let _ = socket
        .broadcast()
        .to(room_id)
        .emit(
            SocketEvent::SetScreenSharingSSC.to_str(),
            &ScreenSharingResponse {
                participant_id: participant_id,
                is_sharing: is_enabled,
                screen_track_id: screen_track_id,
            },
        )
        .await
        .ok();
}

async fn handle_set_hand_raising<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetHandRaisingDto>,
    webrtc_manager: State<WebRTCManager>,
) {
    let client_id = socket.id.to_string();
    let is_enabled = data.is_raising;

    let client = {
        match webrtc_manager.get_client_by_id(&client_id) {
            Ok(client) => client,
            Err(_) => return,
        }
    };
    let client = client.clone();

    let room_id = client.room_id;
    let participant_id = client.participant_id;

    let _ = webrtc_manager
        .set_hand_raising(&client_id, is_enabled)
        .await;

    let _ = socket
        .broadcast()
        .to(room_id)
        .emit(
            SocketEvent::HandRaisingSSC.to_str(),
            &HandleRaisingResponse {
                participant_id: participant_id,
                is_raising: is_enabled,
            },
        )
        .await
        .ok();
}

async fn handle_start_white_board<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<StartWhiteBoardDto>,
) {
}

async fn handle_update_white_board<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<UpdateWhiteBoardDto>,
) {
}

async fn handle_clean_white_board<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<CleanWhiteBoardDto>,
) {
}

async fn handle_set_subscribe_subtitle<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<SetEnabledDto>,
) {
}

async fn handle_leave_room<A: Adapter>(
    socket: SocketRef<A>,
    webrtc_manager: State<WebRTCManager>,
    sfu_service: State<SfuServiceImpl>,
) {
    let _ = _handle_leave_room(socket, webrtc_manager.0, sfu_service.0, false).await;
}

async fn _handle_on_connection(user_id: i32, socket_id: &str, sfu_service: SfuServiceImpl) {
    let _ = sfu_service.create_ccu(socket_id, user_id).await;
}

async fn _handle_leave_room<A: Adapter>(
    socket: SocketRef<A>,
    webrtc_manager: WebRTCManager,
    sfu_service: SfuServiceImpl,
    is_remove_ccu: bool,
) -> Result<WClient, WebRTCError> {
    let socket_id = socket.id.to_string();

    let info = webrtc_manager.leave_room(&socket_id).await?;

    let info_clone = info.clone();
    let room_id = info_clone.room_id.clone();
    let participant_id = info_clone.participant_id.clone();

    let _ = socket
        .broadcast()
        .to(info_clone.room_id)
        .emit(
            SocketEvent::ParticipantHasLeftSSC.to_str(),
            &ParticipantHasLeftResponse {
                target_id: info_clone.participant_id,
            },
        )
        .await
        .ok();

    socket.leave(room_id);

    match participant_id.parse::<i32>() {
        Ok(id) => match sfu_service.delete_participant(id).await {
            Ok(()) => {
                info!("Participant with ID {} deleted", participant_id);
            }
            Err(err) => {
                warn!("Failed to delete participant: {:?}", err);
            }
        },
        Err(e) => {
            warn!("Failed to parse participant_id as i32: {:?}", e);
        }
    };

    if is_remove_ccu {
        let _ = sfu_service.delete_ccu(&socket_id).await;
    }

    Ok(info)
}
