use dotenvy::dotenv;
use std::env;
use time::{Duration, OffsetDateTime};

#[derive(Debug, Clone)]
pub struct EnvConfig {
    pub public_ip: String,
    pub app_port: u16,
    pub db_uri: DbUri,
    pub redis_uri: RedisUri,
    pub typesense: TypesenseConfig,
    pub aws: AwsConfig,
    pub jwt: JwtConfig,
    pub udp_port_range: UdpPortRange,
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
    pub token_expires_at: OffsetDateTime,
    pub refresh_token_expires_at: OffsetDateTime,
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

impl EnvConfig {
    pub fn new() -> Self {
        dotenv().ok();

        Self {
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
                token_expires_at: Self::parse_expiration("AUTH_JWT_TOKEN_EXPIRES_IN", 86400),
                refresh_token_expires_at: Self::parse_expiration(
                    "AUTH_REFRESH_TOKEN_EXPIRES_IN",
                    315360000,
                ),
            },
        }
    }

    fn get_env(var: &str, default: u16) -> u16 {
        env::var(var)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn parse_expiration(var: &str, default: u64) -> OffsetDateTime {
        let val = env::var(var).unwrap_or_else(|_| default.to_string());
        let seconds = if let Some(num) = val.strip_suffix('h') {
            num.parse::<u64>().map(|n| n * 3600).unwrap_or(default)
        } else if let Some(num) = val.strip_suffix('d') {
            num.parse::<u64>().map(|n| n * 86400).unwrap_or(default)
        } else {
            val.parse().unwrap_or(default)
        };
        OffsetDateTime::now_utc() + Duration::seconds(seconds as i64)
    }
}
