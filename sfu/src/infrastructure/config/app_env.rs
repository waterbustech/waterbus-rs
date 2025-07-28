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
            group_id: env::var("DIST_GROUP_ID").unwrap_or_else(|_| "waterbus-group-0".to_string()),
            public_ip: env::var("RTC_EXTERNAL_IP").unwrap_or_else(|_| "".to_string()),
            node_id: Self::get_node_id(),
            etcd_addr: env::var("DIST_ETCD_URI").expect("DIST_ETCD_URI must be set"),
            udp_port_range: UdpPortRange {
                port_min: Self::get_env("RTC_PORT_MIN", 19200),
                port_max: Self::get_env("RTC_PORT_MAX", 19250),
            },
            grpc_configs: GrpcConfigs {
                sfu_host: Self::get_str_env("DIST_SFU_HOST", "http://[::1]".to_owned()),
                sfu_port: Self::get_env("DIST_SFU_PORT", 50051),
                dispatcher_host: Self::get_str_env(
                    "DIST_DISPATCHER_HOST",
                    "http://[::1]".to_owned(),
                ),
                dispatcher_port: Self::get_env("DIST_DISPATCHER_PORT", 50052),
            },
        }
    }

    #[inline]
    fn get_env(var: &str, default: u16) -> u16 {
        env::var(var)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    #[inline]
    fn get_str_env(var: &str, default: String) -> String {
        env::var(var)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    #[inline]
    fn get_node_id() -> String {
        env::var("POD_ID")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(Self::get_random_node_id())
    }

    #[inline]
    fn get_random_node_id() -> String {
        let node_id = format!("sfu-node-{}", nanoid!(12));
        node_id
    }
}
