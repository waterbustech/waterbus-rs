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
use tracing::warn;
use webrtc_manager::{errors::WebRTCError, models::WClient, webrtc_manager::WebRTCManager};

use crate::{
    core::{
        dtos::socket::socket_dto::{
            AnswerSubscribeDto, CandidateDto, CleanWhiteBoardDto, JoinRoomDto,
            PublisherRenegotiationDto, SetCameraTypeDto, SetEnabledDto, SetHandRaisingDto,
            SetScreenSharingDto, StartWhiteBoardDto, SubscribeDto, SubscriberCandidateDto,
            UpdateWhiteBoardDto,
        },
        env::env_config::EnvConfig,
        types::{
            app_channel::{AppChannel, AppEvent},
            enums::socket_event::SocketEvent,
            res::socket_response::ParticipantHasLeftResponse,
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
        .with_state(WebRTCManager::new())
        .with_adapter::<RedisAdapter<_>>(adapter)
        .build_layer();

    let layer = ServiceBuilder::new()
        .layer(CorsLayer::permissive()) // Enable CORS policy
        .layer(layer);

    io.ns("/", on_connect.with(authenticate_middleware)).await?;

    let layer = layer.compat();
    let router = Router::new().hoop(layer).path("/socket.io").get(version);

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
    let socket_id = socket.id.to_string();
    let user_id = user_id.0;
    _handle_on_connection(user_id.0.parse().unwrap(), &socket_id, sfu_service.0).await;

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

    let _ = _handle_leave_room(socket, webrtc_manager.0.clone(), sfu_service.0).await;
}

async fn on_reconnect<A: Adapter>(_: SocketRef<A>) {}

async fn handle_join_room<A: Adapter>(_: SocketRef<A>, Data(_data): Data<JoinRoomDto>) {}

async fn handle_subscribe<A: Adapter>(_: SocketRef<A>, Data(_data): Data<SubscribeDto>) {}

async fn handle_answer_subscribe<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<AnswerSubscribeDto>,
) {
}

async fn handle_publisher_renegotiation<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<PublisherRenegotiationDto>,
) {
}

async fn handle_publisher_candidate<A: Adapter>(_: SocketRef<A>, Data(_data): Data<CandidateDto>) {}

async fn handle_subscriber_candidate<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<SubscriberCandidateDto>,
) {
}

async fn handle_set_e2ee_enabled<A: Adapter>(_: SocketRef<A>, Data(_data): Data<SetEnabledDto>) {}

async fn handle_set_camera_type<A: Adapter>(_: SocketRef<A>, Data(_data): Data<SetCameraTypeDto>) {}

async fn handle_set_video_enabled<A: Adapter>(_: SocketRef<A>, Data(_data): Data<SetEnabledDto>) {}

async fn handle_set_audio_enabled<A: Adapter>(_: SocketRef<A>, Data(_data): Data<SetEnabledDto>) {}

async fn handle_set_screen_sharing<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<SetScreenSharingDto>,
) {
}

async fn handle_set_hand_raising<A: Adapter>(
    _: SocketRef<A>,
    Data(_data): Data<SetHandRaisingDto>,
) {
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

async fn handle_leave_room<A: Adapter>(_: SocketRef<A>) {}

async fn _handle_on_connection(user_id: i32, socket_id: &str, sfu_service: SfuServiceImpl) {
    let _ = sfu_service.create_ccu(socket_id, user_id).await;
}

async fn _handle_leave_room<A: Adapter>(
    socket: SocketRef<A>,
    webrtc_manager: WebRTCManager,
    sfu_service: SfuServiceImpl,
) -> Result<WClient, WebRTCError> {
    let socket_id = socket.id.to_string();

    let info = webrtc_manager.leave_room(&socket_id).await?;

    let info_clone = info.clone();
    let room_id = info_clone.room_id.clone();

    let _ = socket
        .broadcast()
        .to(info_clone.room_id)
        .emit(
            SocketEvent::ParticipantHasLeftSSC.to_str(),
            &ParticipantHasLeftResponse {
                target_id: info_clone.participant_id,
            },
        )
        .await;

    socket.leave(room_id);

    let _ = sfu_service.delete_ccu(&socket_id).await;

    Ok(info)
}
