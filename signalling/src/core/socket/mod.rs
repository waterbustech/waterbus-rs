use std::{str::FromStr, time::Duration};

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
#[cfg(feature = "redis-cluster")]
use socketioxide_redis::drivers::redis::ClusterDriver;
#[cfg(not(feature = "redis-cluster"))]
use socketioxide_redis::drivers::redis::RedisDriver;
use socketioxide_redis::{
    CustomRedisAdapter, RedisAdapterCtr, drivers::redis::redis_client as redis,
};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use waterbus_proto::{
    AddPublisherCandidateRequest, AddSubscriberCandidateRequest, JoinRoomRequest, LeaveRoomRequest,
    MigratePublisherRequest, PublisherRenegotiationRequest, SetCameraType, SetEnabledRequest,
    SetScreenSharingRequest, SetSubscriberSdpRequest, SubscribeHlsLiveStreamRequest,
    SubscribeRequest,
};

use crate::{
    core::{
        dtos::socket::socket_dto::{
            AnswerSubscribeDto, JoinRoomDto, MigrateConnectionDto, PublisherCandidateDto,
            PublisherRenegotiationDto, SetCameraTypeDto, SetEnabledDto, SetHandRaisingDto,
            SetScreenSharingDto, SubscribeDto, SubscribeHlsLiveStreamDto, SubscriberCandidateDto,
        },
        env::app_env::AppEnv,
        types::{
            app_channel::AppEvent,
            enums::ws_event::WsEvent,
            responses::socket_response::{
                CameraTypeResponse, EnabledResponse, HandleRaisingResponse, IceCandidate,
                JoinRoomResponse, NewUserJoinedResponse, ParticipantHasLeftResponse,
                RenegotiateResponse, ScreenSharingResponse, SubscribeHlsLiveStreamResponse,
                SubscribeParticipantResponse, SubscribeResponse, SubscriberRenegotiationResponse,
                SubsriberCandidateResponse,
            },
        },
        utils::jwt_utils::JwtUtils,
    },
    features::{
        room::{
            repository::RoomRepositoryImpl,
            service::{RoomService, RoomServiceImpl},
        },
        user::repository::UserRepositoryImpl,
    },
};

#[cfg(feature = "redis-cluster")]
type DefaultDriver = ClusterDriver;

#[cfg(not(feature = "redis-cluster"))]
type DefaultDriver = RedisDriver;

#[derive(Clone)]
pub struct UserId(pub String);

#[handler(tags("socket.io"))]
async fn version() -> &'static str {
    "[v3] Waterbus Service written in Rust"
}

#[cfg(feature = "redis-cluster")]
#[derive(Clone)]
struct RemoteUserCnt(redis::cluster_async::ClusterConnection);

#[cfg(feature = "redis-cluster")]
impl RemoteUserCnt {
    fn new(conn: redis::cluster_async::ClusterConnection) -> Self {
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

#[cfg(not(feature = "redis-cluster"))]
#[derive(Clone)]
struct RemoteUserCnt(redis::aio::MultiplexedConnection);

#[cfg(not(feature = "redis-cluster"))]
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
    room_service: RoomServiceImpl<RoomRepositoryImpl, UserRepositoryImpl>,
    message_receiver: Receiver<AppEvent>,
) -> Result<Router, Box<dyn std::error::Error>> {
    let (adapter, conn);

    #[cfg(feature = "redis-cluster")]
    {
        let client = redis::cluster::ClusterClient::new(env.clone().redis_uris)
            .expect("Failed to create Redis cluster client");
        adapter = RedisAdapterCtr::new_with_cluster(&client).await?;
        conn = client.get_async_connection().await?;
    }

    #[cfg(not(feature = "redis-cluster"))]
    {
        let client = redis::Client::open(env.clone().redis_uris.first().unwrap().as_str())?;

        adapter = RedisAdapterCtr::new_with_redis(&client).await?;
        conn = client.get_multiplexed_tokio_connection().await?;
    }

    let env_clone = env.clone();

    let (dispacher_sender, dispatcher_receiver) = async_channel::unbounded::<DispatcherCallback>();

    let configs = DispatcherConfigs {
        redis_uris: env_clone.redis_uris,
        etcd_uri: env_clone.etcd_addr,
        dispatcher_port: env_clone.grpc_configs.dispatcher_port,
        sfu_port: env_clone.grpc_configs.sfu_port,
        group_id: env_clone.group_id,
        sender: dispacher_sender,
    };

    let dispatcher = DispatcherManager::new(configs).await;

    let (layer, io);

    #[cfg(feature = "redis-cluster")]
    {
        use socketioxide_redis::ClusterAdapter;

        (layer, io) = SocketIo::builder()
            .with_state(RemoteUserCnt::new(conn))
            .with_state(jwt_utils.clone())
            .with_state(room_service.clone())
            .with_state(dispatcher)
            .with_adapter::<ClusterAdapter<_>>(adapter)
            .with_parser(ParserConfig::msgpack())
            .ping_interval(Duration::from_secs(5))
            .ping_timeout(Duration::from_secs(2))
            .build_layer()
    }

    #[cfg(not(feature = "redis-cluster"))]
    {
        use socketioxide_redis::RedisAdapter;

        (layer, io) = SocketIo::builder()
            .with_state(RemoteUserCnt::new(conn))
            .with_state(jwt_utils.clone())
            .with_state(room_service.clone())
            .with_state(dispatcher)
            .with_adapter::<RedisAdapter<_>>(adapter)
            .with_parser(ParserConfig::msgpack())
            .ping_interval(Duration::from_secs(5))
            .ping_timeout(Duration::from_secs(2))
            .build_layer()
    }

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
        room_service,
    ));

    let io_clone = io.clone();
    tokio::spawn(handle_message_update(io_clone, message_receiver));

    Ok(router)
}

pub async fn handle_dispatcher_callback(
    io: SocketIo<CustomRedisAdapter<Emitter, DefaultDriver>>,
    receiver: Receiver<DispatcherCallback>,
    room_service: RoomServiceImpl<RoomRepositoryImpl, UserRepositoryImpl>,
) {
    // Non-blocking check for any new messages on the channel
    while let Ok(msg) = receiver.recv().await {
        match msg {
            DispatcherCallback::NodeTerminated(node_id) => {
                let _ = room_service.delete_participants_by_node(&node_id).await;
            }
            DispatcherCallback::NewUserJoined(info) => {
                let io = io.clone();
                let room_service = room_service.clone();
                let room_id = info.room_id;
                let participant_id = info.participant_id;
                let client_id = info.client_id;
                let node_id = info.node_id;
                let is_migrate = info.is_migrate;

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
                            let participant = room_service
                                .update_participant(participant_id_parsed, &node_id)
                                .await;

                            if let Ok(participant) = participant {
                                let _ = socket
                                    .broadcast()
                                    .to(room_id)
                                    .emit(
                                        WsEvent::RoomNewParticipant.to_str(),
                                        &NewUserJoinedResponse {
                                            participant,
                                            is_migrate,
                                        },
                                    )
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
                                    WsEvent::RoomSubscriberRenegotiation.to_str(),
                                    &SubscriberRenegotiationResponse { target_id, sdp },
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
                                    .emit(WsEvent::RoomPublisherCandidate.to_str(), &candidate)
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
                                        WsEvent::RoomSubscriberCandidate.to_str(),
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
    io: SocketIo<CustomRedisAdapter<Emitter, DefaultDriver>>,
    receiver: Receiver<AppEvent>,
) {
    // Non-blocking check for any new messages on the channel
    while let Ok(msg) = receiver.recv().await {
        match msg {
            AppEvent::SendMessage(msg) => {
                if let Some(room) = msg.clone().room {
                    let io = io.clone();
                    let msg = msg.clone();
                    let room_id = room.id.to_string();
                    tokio::spawn(async move {
                        let _ = io
                            .broadcast()
                            .to(room_id)
                            .emit(WsEvent::ChatSend.to_str(), &msg)
                            .await
                            .ok();
                    });
                }
            }
            AppEvent::UpdateMessage(msg) => {
                if let Some(room) = msg.clone().room {
                    let io = io.clone();
                    let msg = msg.clone();
                    let room_id = room.id.to_string();
                    tokio::spawn(async move {
                        let _ = io
                            .broadcast()
                            .to(room_id)
                            .emit(WsEvent::ChatUpdate.to_str(), &msg)
                            .await
                            .ok();
                    });
                }
            }
            AppEvent::DeleteMessage(msg) => {
                if let Some(room) = msg.clone().room {
                    let io = io.clone();
                    let msg = msg.clone();
                    let room_id = room.id.to_string();
                    tokio::spawn(async move {
                        let _ = io
                            .broadcast()
                            .to(room_id)
                            .emit(WsEvent::ChatDelete.to_str(), &msg)
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

async fn on_connect<A: Adapter>(socket: SocketRef<A>, user_id: Extension<UserId>) {
    info!("user {:?} connected", user_id.0.0);

    socket.on(WsEvent::RoomReconnect.to_str(), on_reconnect);
    socket.on(WsEvent::RoomPublish.to_str(), handle_join_room);
    socket.on(WsEvent::RoomSubscribe.to_str(), handle_subscribe);
    socket.on(
        WsEvent::RoomSubscribeHlsLiveStream.to_str(),
        handle_subscribe_hls_live_stream,
    );
    socket.on(
        WsEvent::RoomAnswerSubscriber.to_str(),
        handle_answer_subscribe,
    );
    socket.on(
        WsEvent::RoomPublisherRenegotiation.to_str(),
        handle_publisher_renegotiation,
    );
    socket.on(
        WsEvent::RoomPublisherCandidate.to_str(),
        handle_publisher_candidate,
    );
    socket.on(
        WsEvent::RoomSubscriberCandidate.to_str(),
        handle_subscriber_candidate,
    );
    socket.on(WsEvent::RoomMigrate.to_str(), handle_migrate_connection);

    socket.on(WsEvent::RoomCameraType.to_str(), handle_set_camera_type);
    socket.on(WsEvent::RoomVideoEnabled.to_str(), handle_set_video_enabled);
    socket.on(WsEvent::RoomAudioEnabled.to_str(), handle_set_audio_enabled);
    socket.on(
        WsEvent::RoomScreenSharing.to_str(),
        handle_set_screen_sharing,
    );
    socket.on(WsEvent::RoomHandRaising.to_str(), handle_set_hand_raising);
    socket.on(
        WsEvent::RoomSubtitleTrack.to_str(),
        handle_set_subscribe_subtitle,
    );
    socket.on(WsEvent::RoomLeave.to_str(), handle_leave_room);

    socket.on_disconnect(on_disconnect);
}

async fn on_disconnect<A: Adapter>(
    socket: SocketRef<A>,
    user_cnt: State<RemoteUserCnt>,
    dispatcher_manager: State<DispatcherManager>,
    room_service: State<RoomServiceImpl<RoomRepositoryImpl, UserRepositoryImpl>>,
) {
    let _ = _handle_leave_room(socket, dispatcher_manager.0, room_service.0).await;

    let _ = user_cnt.remove_user().await.unwrap_or(0);
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
        connection_type: data.connection_type as i32,
        streaming_protocol: data.streaming_protocol as i32,
        is_ipv6_supported: data.is_ipv6_supported,
    };

    match dispatcher_manager.join_room(req).await {
        Ok(res) => {
            socket.join(room_id.clone());

            if !res.sdp.is_empty() {
                let response = JoinRoomResponse {
                    sdp: res.sdp,
                    is_recording: res.is_recording,
                };

                let _ = socket.emit(WsEvent::RoomPublish.to_str(), &response).ok();
            }
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
    let participant_id = data.participant_id.clone();
    let room_id = data.room_id.clone();

    let req = SubscribeRequest {
        client_id,
        target_id: target_id.clone(),
        participant_id,
        room_id,
        is_ipv6_supported: data.is_ipv6_supported,
    };

    let res = dispatcher_manager.subscribe(req).await;

    if let Ok(res) = res {
        let _ = socket
            .emit(
                WsEvent::RoomAnswerSubscriber.to_str(),
                &SubscribeParticipantResponse {
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
                    target_id,
                },
            )
            .ok();
    }
}

async fn handle_subscribe_hls_live_stream<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SubscribeHlsLiveStreamDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let target_id = data.target_id;
    let room_id = data.room_id;
    let participant_id = data.participant_id;

    let req = SubscribeHlsLiveStreamRequest {
        client_id,
        target_id,
        room_id,
        participant_id,
    };

    let res = dispatcher_manager.subscribe_hls_live_stream(req).await;

    if let Ok(res) = res {
        let _ = socket
            .emit(
                WsEvent::RoomSubscribeHlsLiveStream.to_str(),
                &SubscribeHlsLiveStreamResponse {
                    hls_urls: res.hls_urls,
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
    // P2P handler
    if data.connection_type == 0 {
        let response = JoinRoomResponse {
            sdp: data.sdp,
            is_recording: false,
        };
        let _ = socket
            .broadcast()
            .to(data.room_id)
            .emit(WsEvent::RoomPublish.to_str(), &response)
            .await
            .ok();
    } else {
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
}

async fn handle_publisher_renegotiation<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<PublisherRenegotiationDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    // P2P handler
    if data.connection_type == 0 {
        let _ = socket
            .broadcast()
            .to(data.room_id)
            .emit(
                WsEvent::RoomSubscriberRenegotiation.to_str(),
                &SubscriberRenegotiationResponse {
                    target_id: "".to_string(),
                    sdp: data.sdp,
                },
            )
            .await
            .ok();
    } else {
        let client_id = socket.id.to_string();
        let sdp = data.sdp;

        let req = PublisherRenegotiationRequest { client_id, sdp };

        let sdp = dispatcher_manager.publisher_renegotiate(req).await;

        if let Ok(sdp) = sdp {
            let _ = socket
                .emit(
                    WsEvent::RoomPublisherRenegotiation.to_str(),
                    &RenegotiateResponse { sdp: sdp.sdp },
                )
                .ok();
        }
    }
}

async fn handle_migrate_connection<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<MigrateConnectionDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let sdp = data.sdp;
    let connection_type = data.connection_type as i32;

    let req = MigratePublisherRequest {
        client_id,
        sdp,
        connection_type,
    };

    let sdp = dispatcher_manager.migrate_connection(req).await;

    if let Ok(sdp) = sdp
        && let Some(sdp) = sdp.sdp
    {
        let _ = socket
            .emit(WsEvent::RoomMigrate.to_str(), &RenegotiateResponse { sdp })
            .ok();
    }
}

async fn handle_publisher_candidate<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<PublisherCandidateDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let candidate = data.candidate;

    let candidate = waterbus_proto::common::IceCandidate {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_m_line_index: candidate.sdp_m_line_index.map(|v| v as u32),
    };

    let req = AddPublisherCandidateRequest {
        client_id,
        candidate: Some(candidate.clone()),
        connection_type: data.connection_type as i32,
    };

    if data.connection_type == 0 {
        let _ = socket
            .broadcast()
            .to(data.room_id)
            .emit(
                WsEvent::RoomSubscriberCandidate.to_str(),
                &SubsriberCandidateResponse {
                    candidate: IceCandidate {
                        candidate: candidate.candidate,
                        sdp_mid: candidate.sdp_mid,
                        sdp_m_line_index: candidate.sdp_m_line_index,
                    },
                    target_id: "".to_string(),
                },
            )
            .await
            .ok();
    } else {
        let _ = dispatcher_manager.add_publisher_candidate(req).await;
    }
}

async fn handle_subscriber_candidate<A: Adapter>(
    socket: SocketRef<A>,
    Data(data): Data<SubscriberCandidateDto>,
    dispatcher_manager: State<DispatcherManager>,
) {
    let client_id = socket.id.to_string();
    let candidate = data.candidate.clone();
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
        connection_type: data.connection_type as i32,
    };

    if data.connection_type == 0 {
        let _ = socket
            .broadcast()
            .to(data.room_id)
            .emit(WsEvent::RoomPublisherCandidate.to_str(), &data.candidate)
            .await
            .ok();
    } else {
        let _ = dispatcher_manager.add_subscriber_candidate(req).await;
    }
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

    if let Ok(client) = resp {
        let _ = socket
            .broadcast()
            .to(client.room_id)
            .emit(
                WsEvent::RoomCameraType.to_str(),
                &CameraTypeResponse {
                    participant_id: client.participant_id,
                    type_: camera_type,
                },
            )
            .await
            .ok();
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

    if let Ok(client) = resp {
        let _ = socket
            .broadcast()
            .to(client.room_id)
            .emit(
                WsEvent::RoomVideoEnabled.to_str(),
                &EnabledResponse {
                    participant_id: client.participant_id,
                    is_enabled,
                },
            )
            .await
            .ok();
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

    if let Ok(client) = resp {
        let _ = socket
            .broadcast()
            .to(client.room_id)
            .emit(
                WsEvent::RoomAudioEnabled.to_str(),
                &EnabledResponse {
                    participant_id: client.participant_id,
                    is_enabled,
                },
            )
            .await
            .ok();
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

    if let Ok(client) = resp {
        let _ = socket
            .broadcast()
            .to(client.room_id)
            .emit(
                WsEvent::RoomScreenSharing.to_str(),
                &ScreenSharingResponse {
                    participant_id: client.participant_id,
                    is_sharing: is_enabled,
                    screen_track_id,
                },
            )
            .await
            .ok();
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

    if let Ok(client) = resp {
        let _ = socket
            .broadcast()
            .to(client.room_id)
            .emit(
                WsEvent::RoomHandRaising.to_str(),
                &HandleRaisingResponse {
                    participant_id: client.participant_id,
                    is_raising: is_enabled,
                },
            )
            .await
            .ok();
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
    room_service: State<RoomServiceImpl<RoomRepositoryImpl, UserRepositoryImpl>>,
) {
    let _ = _handle_leave_room(socket, dispatcher_manager.0, room_service.0).await;
}

async fn _handle_leave_room<A: Adapter>(
    socket: SocketRef<A>,
    dispatcher_manager: DispatcherManager,
    room_service: RoomServiceImpl<RoomRepositoryImpl, UserRepositoryImpl>,
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
            WsEvent::RoomParticipantLeft.to_str(),
            &ParticipantHasLeftResponse {
                target_id: info_clone.participant_id,
            },
        )
        .await
        .ok();

    socket.leave(room_id);

    match participant_id.parse::<i32>() {
        Ok(id) => match room_service.delete_participant(id).await {
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

    Ok(())
}
