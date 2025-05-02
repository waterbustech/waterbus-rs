use dotenvy::dotenv;
use nanoid::nanoid;
use std::env;

#[derive(Debug, Clone)]
pub struct AppEnv {
    pub public_ip: String,
    pub node_id: String,
    pub node_ip: String,
    pub etcd_addr: String,
    pub grpc_port: GrpcPort,
    pub udp_port_range: UdpPortRange,
}

#[derive(Debug, Clone)]
pub struct GrpcPort {
    pub sfu_port: u16,
    pub dispatcher_port: u16,
}

#[derive(Debug, Clone)]
pub struct UdpPortRange {
    pub port_min: u16,
    pub port_max: u16,
}

impl AppEnv {
    pub fn new() -> Self {
        dotenv().ok();

        Self {
            public_ip: env::var("PUBLIC_IP").unwrap_or_else(|_| "".to_string()),
            node_id: Self::get_node_id(),
            node_ip: env::var("NODE_IP").expect("NODE_IP must be set"),
            etcd_addr: env::var("ETCD_URI").expect("ETCD_URI must be set"),
            udp_port_range: UdpPortRange {
                port_min: Self::get_env("PORT_MIN_UDP", 19200),
                port_max: Self::get_env("PORT_MAX_UDP", 19250),
            },
            grpc_port: GrpcPort {
                sfu_port: Self::get_env("SFU_GRPC_PORT", 50051),
                dispatcher_port: Self::get_env("DISPATCHER_GRPC_PORT", 50052),
            },
        }
    }

    fn get_env(var: &str, default: u16) -> u16 {
        env::var(var)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn get_node_id() -> String {
        env::var("POD_ID")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(Self::get_random_node_id())
    }

    fn get_random_node_id() -> String {
        let node_id = format!("sfu-node-{}", nanoid!(12));
        node_id
    }
}
