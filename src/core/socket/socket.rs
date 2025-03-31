use salvo::prelude::*;
use socketioxide::{
    SocketIo,
    adapter::Adapter,
    extract::{Extension, SocketRef, State},
};
use socketioxide_redis::{RedisAdapter, RedisAdapterCtr, drivers::redis::redis_client as redis};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

#[handler]
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

pub async fn get_socket_router() -> Result<Router, Box<dyn std::error::Error>> {
    let client = redis::Client::open("redis://127.0.0.1:6379?protocol=resp3")?;
    let adapter = RedisAdapterCtr::new_with_redis(&client).await?;
    let conn = client.get_multiplexed_tokio_connection().await?;

    let (layer, io) = SocketIo::builder()
        .with_state(RemoteUserCnt::new(conn))
        .with_adapter::<RedisAdapter<_>>(adapter)
        .build_layer();

    let layer = ServiceBuilder::new()
        .layer(CorsLayer::permissive()) // Enable CORS policy
        .layer(layer);

    io.ns("/", on_connect).await?;

    let layer = layer.compat();
    let router = Router::with_path("/socket.io").hoop(layer).goal(version);

    Ok(router)
}

async fn on_connect<A: Adapter>(socket: SocketRef<A>) {
    socket.on_disconnect(on_disconnect);
}

async fn on_disconnect<A: Adapter>(
    s: SocketRef<A>,
    user_cnt: State<RemoteUserCnt>,
    Extension(username): Extension<String>,
) {
    // let num_users = user_cnt.remove_user().await.unwrap_or(0);
    // let res = &Res::UserEvent {
    //     num_users,
    //     username,
    // };
}
