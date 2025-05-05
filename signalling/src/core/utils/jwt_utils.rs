use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use salvo::Handler;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::core::env::app_env::AppEnv;
use crate::core::types::res::failed_response::FailedResponse;

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub id: String,
    pub exp: i64,
}

#[derive(Debug, Clone)]
pub struct JwtUtils {
    secret_key: String,
    refresh_secret_key: String,
    token_duration: time::Duration,
    refresh_token_duration: time::Duration,
}

impl JwtUtils {
    pub fn new(env: AppEnv) -> Self {
        Self {
            secret_key: env.jwt.jwt_token,
            refresh_secret_key: env.jwt.refresh_token,
            token_duration: time::Duration::seconds(env.jwt.token_expires_in_seconds),
            refresh_token_duration: time::Duration::seconds(
                env.jwt.refresh_token_expires_in_seconds,
            ),
        }
    }

    pub fn generate_token(&self, user_id: &str) -> String {
        let exp = OffsetDateTime::now_utc() + self.token_duration;

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
        let exp = OffsetDateTime::now_utc() + self.refresh_token_duration;

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

    pub fn auth_middleware(&self) -> impl Handler {
        #[handler]
        async fn middleware(req: &mut Request, depot: &mut Depot, res: &mut Response) {
            let token = req
                .headers()
                .get("Authorization")
                .and_then(|h| h.to_str().ok());

            let jwt_utils = depot.obtain::<JwtUtils>().unwrap();

            if let Some(token) = token {
                let token = token.trim_start_matches("Bearer ");
                match jwt_utils.decode_token(token) {
                    Ok(claims) => {
                        depot.insert("user_id", claims.id.clone());
                    }
                    Err(_) => {
                        res.status_code(StatusCode::UNAUTHORIZED);
                        return res.render(Json(FailedResponse {
                            message: "Failed to decode token".to_string(),
                        }));
                    }
                }
            } else {
                res.status_code(StatusCode::UNAUTHORIZED);
                return res.render(Json(FailedResponse {
                    message: "Missing bearer token".to_string(),
                }));
            }
        }
        middleware
    }

    pub fn refresh_token_middleware(&self) -> impl Handler {
        #[handler]
        async fn middleware(req: &mut Request, depot: &mut Depot, res: &mut Response) {
            let token = req
                .headers()
                .get("Authorization")
                .and_then(|h| h.to_str().ok());

            let jwt_utils = depot.obtain::<JwtUtils>().unwrap();

            if let Some(token) = token {
                let token = token.trim_start_matches("Bearer ");
                match jwt_utils.decode_refresh_token(token) {
                    Ok(claims) => {
                        depot.insert("user_id", claims.id.clone());
                    }
                    Err(_) => {
                        res.status_code(StatusCode::UNAUTHORIZED);
                        return res.render(Json(FailedResponse {
                            message: "Failed to decode token".to_string(),
                        }));
                    }
                }
            } else {
                res.status_code(StatusCode::UNAUTHORIZED);
                return res.render(Json(FailedResponse {
                    message: "Missing bearer token".to_string(),
                }));
            }
        }
        middleware
    }
}
