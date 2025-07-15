use dotenvy::dotenv;
use std::env;

#[derive(Debug, Clone)]
pub struct AppEnv {
    pub group_id: String,
    pub etcd_addr: String,
    pub public_ip: String,
    pub app_port: u16,
    pub client_api_key: String,
    pub db_uri: DbUri,
    pub redis_uris: Vec<String>,
    pub jwt: JwtConfig,
    pub udp_port_range: UdpPortRange,
    pub grpc_configs: GrpcConfigs,
    pub tls_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct DbUri(pub String);

#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub jwt_token: String,
    pub refresh_token: String,
    pub token_expires_in_seconds: i64,
    pub refresh_token_expires_in_seconds: i64,
}

#[derive(Debug, Clone)]
pub struct UdpPortRange {
    pub port_min: u16,
    pub port_max: u16,
}

#[derive(Debug, Clone)]
pub struct GrpcConfigs {
    pub sfu_host: String,
    pub sfu_port: u16,
    pub dispatcher_host: String,
    pub dispatcher_port: u16,
}

impl Default for AppEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl AppEnv {
    pub fn new() -> Self {
        dotenv().ok();

        let default_urls = vec![
            "redis://127.0.0.1:6379?protocol=resp3",
            "redis://127.0.0.1:6380?protocol=resp3",
            "redis://127.0.0.1:6381?protocol=resp3",
            "redis://127.0.0.1:6382?protocol=resp3",
            "redis://127.0.0.1:6383?protocol=resp3",
            "redis://127.0.0.1:6384?protocol=resp3",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

        let redis_uris = env::var("REDIS_URIS")
            .map(|val| {
                val.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<String>>()
            })
            .unwrap_or(default_urls);

        Self {
            group_id: env::var("GROUP_ID").unwrap_or_else(|_| "waterbus-group-1".to_string()),
            etcd_addr: env::var("ETCD_URI").expect("ETCD_URI must be set"),
            public_ip: env::var("PUBLIC_IP").unwrap_or_else(|_| "".to_string()),
            app_port: Self::get_env("APP_PORT", 3000),
            client_api_key: env::var("CLIENT_SECRET_KEY").unwrap_or_else(|_| "".to_string()),
            udp_port_range: UdpPortRange {
                port_min: Self::get_env("PORT_MIN_UDP", 19000),
                port_max: Self::get_env("PORT_MAX_UDP", 60000),
            },
            db_uri: DbUri(env::var("DATABASE_URL").expect("DATABASE_URL must be set")),
            redis_uris,
            jwt: JwtConfig {
                jwt_token: env::var("AUTH_JWT_SECRET").expect("AUTH_JWT_SECRET must be set"),
                refresh_token: env::var("AUTH_REFRESH_SECRET")
                    .expect("AUTH_REFRESH_SECRET must be set"),
                token_expires_in_seconds: Self::get_dur_env("AUTH_JWT_TOKEN_EXPIRES_IN", 86_400), // a day
                refresh_token_expires_in_seconds: Self::get_dur_env(
                    "AUTH_REFRESH_TOKEN_EXPIRES_IN",
                    31_536_000, // a year
                ),
            },
            grpc_configs: GrpcConfigs {
                sfu_host: Self::get_str_env("SFU_HOST", "http://[::1]".to_owned()),
                sfu_port: Self::get_env("SFU_PORT", 50051),
                dispatcher_host: Self::get_str_env("DISPATCHER_HOST", "http://[::1]".to_owned()),
                dispatcher_port: Self::get_env("DISPATCHER_PORT", 50052),
            },
            tls_enabled: std::env::var("TLS_ENABLED")
                .unwrap_or_else(|_| "false".into())
                .to_lowercase()
                == "true",
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

    fn get_dur_env(var: &str, default: i64) -> i64 {
        env::var(var)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }
}
