use anyhow::anyhow;
use salvo::prelude::*;
use socketioxide::{
    SocketIo,
    adapter::Adapter,
    extract::{SocketRef, State},
    handler::ConnectHandler,
};
use socketioxide_redis::{RedisAdapter, RedisAdapterCtr, drivers::redis::redis_client as redis};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::warn;

use crate::core::{env::env_config::EnvConfig, utils::jwt_utils::JwtUtils};

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
) -> Result<Router, Box<dyn std::error::Error>> {
    let client = redis::Client::open(env.clone().redis_uri.0)?;
    let adapter = RedisAdapterCtr::new_with_redis(&client).await?;
    let conn = client.get_multiplexed_tokio_connection().await?;

    let (layer, io) = SocketIo::builder()
        .with_state(RemoteUserCnt::new(conn))
        .with_state(jwt_utils.clone())
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

async fn on_connect<A: Adapter>(socket: SocketRef<A>) {
    socket.on_disconnect(on_disconnect);
}

async fn on_disconnect<A: Adapter>(_: SocketRef<A>, user_cnt: State<RemoteUserCnt>) {
    let _ = user_cnt.remove_user().await.unwrap_or(0);
}
