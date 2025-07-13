use dotenvy::dotenv;
use nanoid::nanoid;
use std::env;

#[derive(Debug, Clone)]
pub struct AppEnv {
    pub group_id: String,
    pub public_ip: String,
    pub node_id: String,
    pub etcd_addr: String,
    pub grpc_configs: GrpcConfigs,
    pub udp_port_range: UdpPortRange,
}

#[derive(Debug, Clone)]
pub struct GrpcConfigs {
    pub sfu_host: String,
    pub sfu_port: u16,
    pub dispatcher_host: String,
    pub dispatcher_port: u16,
}

#[derive(Debug, Clone)]
pub struct UdpPortRange {
    pub port_min: u16,
    pub port_max: u16,
}

impl Default for AppEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl AppEnv {
    pub fn new() -> Self {
        dotenv().ok();

        Self {
            group_id: env::var("GROUP_ID").unwrap_or_else(|_| "waterbus-group-1".to_string()),
            public_ip: env::var("PUBLIC_IP").unwrap_or_else(|_| "".to_string()),
            node_id: Self::get_node_id(),
            etcd_addr: env::var("ETCD_URI").expect("ETCD_URI must be set"),
            udp_port_range: UdpPortRange {
                port_min: Self::get_env("PORT_MIN_UDP", 19200),
                port_max: Self::get_env("PORT_MAX_UDP", 19250),
            },
            grpc_configs: GrpcConfigs {
                sfu_host: Self::get_str_env("SFU_HOST", "http://[::1]".to_owned()),
                sfu_port: Self::get_env("SFU_PORT", 50051),
                dispatcher_host: Self::get_str_env("DISPATCHER_HOST", "http://[::1]".to_owned()),
                dispatcher_port: Self::get_env("DISPATCHER_PORT", 50052),
            },
        }
    }

    fn get_env(var: &str, default: u16) -> u16 {
        env::var(var)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn get_str_env(var: &str, default: String) -> String {
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
