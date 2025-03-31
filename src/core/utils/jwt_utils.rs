use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use salvo::{
    jwt_auth::{ConstDecoder, HeaderFinder},
    prelude::JwtAuth,
};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::core::env::env_config::EnvConfig;

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub id: String,
    pub exp: i64,
}

#[derive(Debug, Clone)]
pub struct JwtUtils {
    secret_key: String,
    refresh_secret_key: String,
}

impl JwtUtils {
    pub fn new(env: EnvConfig) -> Self {
        Self {
            secret_key: env.jwt.jwt_token,
            refresh_secret_key: env.jwt.refresh_token,
        }
    }

    pub fn generate_token(&self, user_id: &str, expires_in_days: i64) -> String {
        let exp = OffsetDateTime::now_utc() + Duration::days(expires_in_days);
        let claims = JwtClaims {
            id: user_id.to_owned(),
            exp: exp.unix_timestamp(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret_key.as_bytes()),
        )
        .expect("Failed to generate token")
    }

    pub fn decode_token(&self, token: &str) -> Result<JwtClaims, jsonwebtoken::errors::Error> {
        let token_data = decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(self.secret_key.as_bytes()),
            &Validation::default(),
        )?;
        Ok(token_data.claims)
    }

    pub fn generate_refresh_token(&self, user_id: &str) -> String {
        let exp = OffsetDateTime::now_utc() + Duration::days(30);
        let claims = JwtClaims {
            id: user_id.to_owned(),
            exp: exp.unix_timestamp(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.refresh_secret_key.as_bytes()),
        )
        .expect("Failed to generate refresh token")
    }

    pub fn decode_refresh_token(
        &self,
        token: &str,
    ) -> Result<JwtClaims, jsonwebtoken::errors::Error> {
        let token_data = decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(self.refresh_secret_key.as_bytes()),
            &Validation::default(),
        )?;
        Ok(token_data.claims)
    }

    pub fn auth_middleware() -> JwtAuth<JwtClaims, ConstDecoder> {
        JwtAuth::new(ConstDecoder::from_secret("YOUR_SECRET_KEY".as_bytes()))
            .finders(vec![Box::new(HeaderFinder::new())])
    }
}
