use std::str::FromStr;

use anyhow::anyhow;
use async_channel::Receiver;
use dispatcher::{
    dispatcher_manager::{DispatcherConfigs, DispatcherManager},
    domain::DispatcherCallback,
};
use salvo::prelude::*;
use socketioxide::{
    ParserConfig, SocketIo,
    adapter::{Adapter, Emitter},
    extract::{Data, Extension, SocketRef, State},
    handler::ConnectHandler,
    socket::Sid,
};
use socketioxide_redis::{
    CustomRedisAdapter, RedisAdapter, RedisAdapterCtr,
    drivers::redis::{RedisDriver, redis_client as redis},
};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use waterbus_proto::{
    AddPublisherCandidateRequest, AddSubscriberCandidateRequest, JoinRoomRequest, LeaveRoomRequest,
    PublisherRenegotiationRequest, SetCameraType, SetEnabledRequest, SetScreenSharingRequest,
    SetSubscriberSdpRequest, SubscribeRequest,
};

use crate::{
    core::{
        dtos::socket::socket_dto::{
            AnswerSubscribeDto, CandidateDto, JoinRoomDto, MsgDto, PublisherRenegotiationDto,
            SetCameraTypeDto, SetEnabledDto, SetHandRaisingDto, SetScreenSharingDto, SubscribeDto,
            SubscriberCandidateDto,
        },
        env::app_env::AppEnv,
        types::{
            app_channel::AppEvent,
            enums::socket_event::SocketEvent,
            res::socket_response::{
                CameraTypeResponse, EnabledResponse, HandleRaisingResponse, IceCandidate,
                MeetingJoinResponse, MeetingSubscribeResponse, ParticipantHasLeftResponse,
                ScreenSharingResponse, SubscribeResponse, SubscriberRenegotiationResponse,
                SubsriberCandidateResponse,
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
    env: &AppEnv,
    jwt_utils: JwtUtils,
    sfu_service: SfuServiceImpl,
    message_receiver: Receiver<AppEvent>,
) -> Result<Router, Box<dyn std::error::Error>> {
    let client = redis::Client::open(env.clone().redis_uri.0)?;
    let adapter = RedisAdapterCtr::new_with_redis(&client).await?;
    let conn = client.get_multiplexed_tokio_connection().await?;

    let env_clone = env.clone();

    let (dispacher_sender, dispatcher_receiver) = async_channel::unbounded::<DispatcherCallback>();

    let configs = DispatcherConfigs {
        redis_uri: env_clone.redis_uri.0,
        etcd_uri: env_clone.etcd_addr,
        dispatcher_port: env_clone.grpc_port.dispatcher_port,
        sfu_port: env_clone.grpc_port.sfu_port,
        sender: dispacher_sender,
    };

    let dispatcher = DispatcherManager::new(configs).await;

    let (layer, io) = SocketIo::builder()
        .with_state(RemoteUserCnt::new(conn))
        .with_state(jwt_utils.clone())
        .with_state(sfu_service.clone())
        .with_state(dispatcher)
        .with_adapter::<RedisAdapter<_>>(adapter)
        .with_parser(ParserConfig::msgpack())
        .build_layer();

    let layer = ServiceBuilder::new()
        .layer(CorsLayer::permissive()) // Enable CORS policy
        .layer(layer);

    io.ns("/", on_connect.with(authenticate_middleware)).await?;

    let layer = layer.compat();
    let router = Router::new().hoop(layer).path("/socket.io").goal(version);

    // Listener
    let io_clone = io.clone();
    tokio::spawn(handle_dispatcher_callback(
        io_clone,
        dispatcher_receiver,
        sfu_service,
    ));

    let io_clone = io.clone();
    tokio::spawn(handle_message_update(io_clone, message_receiver));

    Ok(router)
}

pub async fn handle_dispatcher_callback(
    io: SocketIo<CustomRedisAdapter<Emitter, RedisDriver>>,
    receiver: Receiver<DispatcherCallback>,
    sfu_service: SfuServiceImpl,
) {
    // Non-blocking check for any new messages on the channel
    while let Ok(msg) = receiver.recv().await {
        match msg {
            DispatcherCallback::NewUserJoined(info) => {
                let io = io.clone();
                let sfu_service = sfu_service.clone();
                let room_id = info.room_id;
                let participant_id = info.participant_id;
                let client_id = info.client_id;

                let participant_id_parsed = match participant_id.parse::<i32>() {
                    Ok(id) => id,
                    Err(e) => {
                        warn!("Failed to parse participant_id as i32: {:?}", e);
                        return;
                    }
                };

                let sid = Sid::from_str(&client_id);

                if let Ok(sid) = sid {
                    if let Some(socket) = io.get_socket(sid) {
                        tokio::spawn(async move {
                            let participant = sfu_service
                                .update_participant(participant_id_parsed, &client_id)
                                .await;

                            if let Ok(participant) = participant {
                                let _ = socket
                                    .broadcast()
                                    .to(room_id)
                                    .emit(SocketEvent::NewParticipantSSC.to_str(), &participant)
                                    .await
                                    .ok();
                            }
                        });
                    } else {
                        warn!("Socket with id {} not found", client_id);
                    }
                }
            }
            DispatcherCallback::SubscriberRenegotiate(info) => {
                let io = io.clone();
                let client_id = info.client_id;
                let target_id = info.target_id;
                let sdp = info.sdp;

                let sid = Sid::from_str(&client_id);

                match sid {
                    Ok(sid) => {
                        if let Some(socket) = io.get_socket(sid) {
                            let _ = socket
                                .emit(
                                    SocketEvent::SubscriberRenegotiationSSC.to_str(),
                                    &SubscriberRenegotiationResponse {
                                        target_id: target_id,
                                        sdp: sdp,
                                    },
                                )
                                .ok();
                        } else {
                            warn!("Socket with id {} not found", client_id);
                        }
                    }
                    Err(err) => warn!("Failed to parse Sid from str: {:?}", err),
                }
            }
            DispatcherCallback::PublisherCandidate(info) => {
                if let Some(candidate) = info.candidate {
                    let io = io.clone();
                    let client_id = info.client_id;

                    let candidate = IceCandidate {
                        candidate: candidate.candidate,
                        sdp_mid: candidate.sdp_mid,
                        sdp_m_line_index: candidate.sdp_m_line_index,
                    };

                    let sid = Sid::from_str(&client_id);

                    match sid {
                        Ok(sid) => {
                            if let Some(socket) = io.get_socket(sid) {
                                let _ = socket
                                    .emit(SocketEvent::PublisherCandidateSSC.to_str(), &candidate)
                                    .ok();
                            } else {
                                warn!("Socket with id {} not found", client_id);
                            }
                        }
                        Err(err) => warn!("Failed to parse Sid from str: {:?}", err),
                    }
                }
            }
            DispatcherCallback::SubscriberCandidate(info) => {
                if let Some(candidate) = info.candidate {
                    let io = io.clone();
                    let client_id = info.client_id;
                    let target_id = info.target_id;

                    let candidate = IceCandidate {
                        candidate: candidate.candidate,
                        sdp_mid: candidate.sdp_mid,
                        sdp_m_line_index: candidate.sdp_m_line_index,
                    };

                    let sid = Sid::from_str(&client_id);

                    match sid {
                        Ok(sid) => {
                            if let Some(socket) = io.get_socket(sid) {
                                let _ = socket
                                    .emit(
                                        SocketEvent::SubscriberCandidateSSC.to_str(),
                                        &SubsriberCandidateResponse {
                                            candidate,
                                            target_id,
                                        },
                                    )
                                    .ok();
                            } else {
                                warn!("Socket with id {} not found", client_id);
                            }
                        }
                        Err(err) => warn!("Failed to parse Sid from str: {:?}", err),
                    }
                }
            }
        }
    }
}

pub async fn handle_message_update(
    io: SocketIo<CustomRedisAdapter<Emitter, RedisDriver>>,
    receiver: Receiver<AppEvent>,
) {
    // Non-blocking check for any new messages on the channel
    while let Ok(msg) = receiver.recv().await {
        match msg {
            AppEvent::SendMessage(msg) => {
                if let Some(meeting) = msg.clone().meeting {
                    let io = io.clone();
                    let msg = msg.clone();
                    let meeting_code = meeting.code.to_string();
                    tokio::spawn(async move {
                        let _ = io
                            .broadcast()
                            .to(meeting_code)
                            .emit(SocketEvent::SendMessageSSC.to_str(), &msg)
                            .await
                            .ok();
                    });
                }
            }
            AppEvent::UpdateMessage(msg) => {
                if let Some(meeting) = msg.clone().meeting {
                    let io = io.clone();
                    let msg = msg.clone();
                    let meeting_code = meeting.code.to_string();
                    tokio::spawn(async move {
                        let _ = io
                            .broadcast()
                            .to(meeting_code)
                            .emit(SocketEvent::UpdateMessageSSC.to_str(), &msg)
                            .await
                            .ok();
                    });
                }
            }
            AppEvent::DeleteMessage(msg) => {
                if let Some(meeting) = msg.clone().meeting {
                    let io = io.clone();
                    let msg = msg.clone();
                    let meeting_code = meeting.code.to_string();
                    tokio::spawn(async move {
                        let _ = io
                            .broadcast()
                            .to(meeting_code)
                            .emit(SocketEvent::DeleteMessageSSC.to_str(), &msg)
                            .await
                            .ok();
                    });
                }
            }
        }
    }
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
        SocketEvent::SetSubscribeSubtitleCSS.to_str(),
        handle_set_subscribe_subtitle,
    );
    socket.on(SocketEvent::LeaveRoomCSS.to_str(), handle_leave_room);

    socket.on_disconnect(on_disconnect);
}

async fn on_disconnect<A: Adapter>(
    socket: SocketRef<A>,
    user_cnt: State<RemoteUserCnt>,
    dispatcher_manager: State<DispatcherManager>,
    sfu_service: State<SfuServiceImpl>,
) {
    let _ = user_cnt.remove_user().await.unwrap_or(0);

    let _ = _handle_leave_room(socket, dispatcher_manager.0.clone(), sfu_service.0, true).await;
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
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let participant_id = &data.participant_id;
    let room_id = data.room_id.clone();

    let req = JoinRoomRequest {
        sdp: data.sdp,
        is_audio_enabled: data.is_audio_enabled,
        is_video_enabled: data.is_video_enabled,
        is_e2ee_enabled: data.is_e2ee_enabled,
        total_tracks: data.total_tracks as i32,
        client_id,
        participant_id: participant_id.to_string(),
        room_id: room_id.clone(),
    };

    match dispatcher_manager.join_room(req).await {
        Ok(res) => {
            socket.join(room_id.clone());

            let response = MeetingJoinResponse {
                sdp: res.sdp,
                is_recording: res.is_recording,
            };

            let _ = socket
                .emit(SocketEvent::PublishSSC.to_str(), &response)
                .ok();
        }
        Err(err) => {
            warn!("Err: {:?}", err)
        }
    }
}

async fn handle_subscribe<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SubscribeDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let target_id = data.target_id;

    let req = SubscribeRequest {
        client_id,
        target_id: target_id.clone(),
    };

    let res = dispatcher_manager.subscribe(req).await;

    if let Ok(res) = res {
        let _ = socket
            .emit(
                SocketEvent::AnswerSubscriberSSC.to_str(),
                &MeetingSubscribeResponse {
                    subscribe_response: SubscribeResponse {
                        offer: res.offer,
                        camera_type: res.camera_type as u8,
                        video_enabled: res.video_enabled,
                        audio_enabled: res.audio_enabled,
                        is_screen_sharing: res.is_screen_sharing,
                        is_hand_raising: res.is_hand_raising,
                        is_e2ee_enabled: res.is_e2ee_enabled,
                        video_codec: res.video_codec,
                        screen_track_id: res.screen_track_id,
                    },
                    target_id: target_id,
                },
            )
            .ok();
    }
}

async fn handle_answer_subscribe<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<AnswerSubscribeDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let target_id = data.target_id;
    let sdp = data.sdp;

    let req = SetSubscriberSdpRequest {
        client_id,
        target_id,
        sdp,
    };

    let _ = dispatcher_manager.set_subscribe_sdp(req).await;
}

async fn handle_publisher_renegotiation<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<PublisherRenegotiationDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let sdp = data.sdp;

    let req = PublisherRenegotiationRequest { client_id, sdp };

    let sdp = dispatcher_manager.publisher_renegotiate(req).await;

    match sdp {
        Ok(sdp) => {
            let _ = socket
                .emit(
                    SocketEvent::PublisherRenegotiationSSC.to_str(),
                    &PublisherRenegotiationDto { sdp: sdp.sdp },
                )
                .ok();
        }
        Err(_) => {}
    }
}

async fn handle_publisher_candidate<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<CandidateDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let candidate = data;

    let candidate = waterbus_proto::common::IceCandidate {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_m_line_index: candidate.sdp_m_line_index.map(|v| v as u32),
    };

    let req = AddPublisherCandidateRequest {
        client_id,
        candidate: Some(candidate),
    };

    let _ = dispatcher_manager.add_publisher_candidate(req).await;
}

async fn handle_subscriber_candidate<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SubscriberCandidateDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let candidate = data.candidate;
    let target_id = data.target_id;

    let candidate = waterbus_proto::common::IceCandidate {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_m_line_index: candidate.sdp_m_line_index.map(|v| v as u32),
    };

    let req = AddSubscriberCandidateRequest {
        client_id,
        target_id,
        candidate: Some(candidate),
    };

    let _ = dispatcher_manager.add_subscriber_candidate(req).await;
}

async fn handle_set_camera_type<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetCameraTypeDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let camera_type = data.type_;

    let req = SetCameraType {
        camera_type,
        client_id,
    };

    let resp = dispatcher_manager.set_camera_type(req).await;

    match resp {
        Ok(client) => {
            let _ = socket
                .broadcast()
                .to(client.room_id)
                .emit(
                    SocketEvent::SetCameraTypeSSC.to_str(),
                    &CameraTypeResponse {
                        participant_id: client.participant_id,
                        type_: camera_type,
                    },
                )
                .await
                .ok();
        }
        Err(_) => {}
    }
}

async fn handle_set_video_enabled<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetEnabledDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let is_enabled = data.is_enabled;

    let req = SetEnabledRequest {
        client_id,
        is_enabled,
    };

    let resp = dispatcher_manager.set_video_enabled(req).await;

    match resp {
        Ok(client) => {
            let _ = socket
                .broadcast()
                .to(client.room_id)
                .emit(
                    SocketEvent::SetVideoEnabledSSC.to_str(),
                    &EnabledResponse {
                        participant_id: client.participant_id,
                        is_enabled,
                    },
                )
                .await
                .ok();
        }
        Err(_) => {}
    }
}

async fn handle_set_audio_enabled<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetEnabledDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let is_enabled = data.is_enabled;

    let req = SetEnabledRequest {
        client_id,
        is_enabled,
    };

    let resp = dispatcher_manager.set_audio_enabled(req).await;

    match resp {
        Ok(client) => {
            let _ = socket
                .broadcast()
                .to(client.room_id)
                .emit(
                    SocketEvent::SetAudioEnabledSSC.to_str(),
                    &EnabledResponse {
                        participant_id: client.participant_id,
                        is_enabled,
                    },
                )
                .await
                .ok();
        }
        Err(_) => {}
    }
}

async fn handle_set_screen_sharing<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetScreenSharingDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let is_enabled = data.is_sharing;
    let screen_track_id = data.screen_track_id;

    let req = SetScreenSharingRequest {
        client_id,
        is_enabled,
        screen_track_id: screen_track_id.clone(),
    };

    let resp = dispatcher_manager.set_screen_sharing(req).await;

    match resp {
        Ok(client) => {
            let _ = socket
                .broadcast()
                .to(client.room_id)
                .emit(
                    SocketEvent::SetScreenSharingSSC.to_str(),
                    &ScreenSharingResponse {
                        participant_id: client.participant_id,
                        is_sharing: is_enabled,
                        screen_track_id: screen_track_id,
                    },
                )
                .await
                .ok();
        }
        Err(_) => {}
    }
}

async fn handle_set_hand_raising<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SetHandRaisingDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let is_enabled = data.is_raising;

    let req = SetEnabledRequest {
        client_id,
        is_enabled,
    };

    let resp = dispatcher_manager.set_hand_raising(req).await;

    match resp {
        Ok(client) => {
            let _ = socket
                .broadcast()
                .to(client.room_id)
                .emit(
                    SocketEvent::HandRaisingSSC.to_str(),
                    &HandleRaisingResponse {
                        participant_id: client.participant_id,
                        is_raising: is_enabled,
                    },
                )
                .await
                .ok();
        }
        Err(_) => {}
    }
}

async fn handle_set_subscribe_subtitle<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<SetEnabledDto>,
) {
}

async fn handle_leave_room<A: Adapter>(
    socket: SocketRef<A>,
    dispatcher_manager: State<DispatcherManager>,
    sfu_service: State<SfuServiceImpl>,
) {
    let _ = _handle_leave_room(socket, dispatcher_manager.0, sfu_service.0, false).await;
}

async fn _handle_on_connection(user_id: i32, socket_id: &str, sfu_service: SfuServiceImpl) {
    let _ = sfu_service.create_ccu(socket_id, user_id).await;
}

async fn _handle_leave_room<A: Adapter>(
    socket: SocketRef<A>,
    dispatcher_manager: DispatcherManager,
    sfu_service: SfuServiceImpl,
    is_remove_ccu: bool,
) -> Result<(), anyhow::Error> {
    let client_id = socket.id.to_string();

    let req = LeaveRoomRequest { client_id };

    let info = dispatcher_manager.leave_room(req).await?;

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
        let _ = sfu_service.delete_ccu(&socket.id.to_string()).await;
    }

    Ok(())
}
