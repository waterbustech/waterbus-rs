use dotenvy::dotenv;
use std::env;

#[derive(Debug, Clone)]
pub struct AppEnv {
    pub etcd_addr: String,
    pub public_ip: String,
    pub app_port: u16,
    pub db_uri: DbUri,
    pub redis_uri: RedisUri,
    pub typesense: TypesenseConfig,
    pub aws: AwsConfig,
    pub jwt: JwtConfig,
    pub udp_port_range: UdpPortRange,
    pub grpc_configs: GrpcConfigs,
}

#[derive(Debug, Clone)]
pub struct DbUri(pub String);

#[derive(Debug, Clone)]
pub struct RedisUri(pub String);

#[derive(Debug, Clone)]
pub struct TypesenseConfig {
    pub uri: String,
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub jwt_token: String,
    pub refresh_token: String,
    pub token_expires_in_seconds: i64,
    pub refresh_token_expires_in_seconds: i64,
}

#[derive(Debug, Clone)]
pub struct AwsConfig {
    pub key_id: String,
    pub access_key: String,
    pub region: String,
    pub bucket_name: String,
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

impl AppEnv {
    pub fn new() -> Self {
        dotenv().ok();

        Self {
            etcd_addr: env::var("ETCD_URI").expect("ETCD_URI must be set"),
            public_ip: env::var("PUBLIC_IP").unwrap_or_else(|_| "".to_string()),
            app_port: Self::get_env("APP_PORT", 3000),
            udp_port_range: UdpPortRange {
                port_min: Self::get_env("PORT_MIN_UDP", 19200),
                port_max: Self::get_env("PORT_MAX_UDP", 19250),
            },
            db_uri: DbUri(env::var("DATABASE_URI").expect("DATABASE_URI must be set")),
            redis_uri: RedisUri(env::var("REDIS_URI").expect("REDIS_URI must be set")),
            typesense: TypesenseConfig {
                uri: env::var("TYPESENSE_URI").expect("TYPESENSE_URI must be set"),
                api_key: env::var("TYPESENSE_API_KEY").expect("TYPESENSE_API_KEY must be set"),
            },
            aws: AwsConfig {
                key_id: env::var("AWS_ACCESS_KEY_ID").expect("AWS_ACCESS_KEY_ID must be set"),
                access_key: env::var("AWS_SECRET_ACCESS_KEY")
                    .expect("AWS_SECRET_ACCESS_KEY must be set"),
                region: env::var("AWS_REGION").expect("AWS_REGION must be set"),
                bucket_name: env::var("AWS_S3_BUCKET_NAME")
                    .expect("AWS_S3_BUCKET_NAME must be set"),
            },
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
