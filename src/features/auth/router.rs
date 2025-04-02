use std::time::Duration;

use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::ObjectCannedAcl;
use reqwest::{Client, StatusCode};
use salvo::oapi::extract::JsonBody;
use salvo::prelude::*;
use salvo::{Response, Router, oapi::endpoint};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::api::salvo_config::DbConnection;
use crate::core::dtos::auth::login_dto::LoginDto;
use crate::core::dtos::auth::oauth_dto::OauthRequestDto;
use crate::core::env::env_config::EnvConfig;
use crate::core::types::res::failed_response::FailedResponse;
use crate::core::utils::aws_utils::get_s3_client;
use crate::core::utils::jwt_utils::JwtUtils;

use super::repository::AuthRepositoryImpl;
use super::service::{AuthService, AuthServiceImpl};

#[derive(Serialize, Deserialize)]
struct OauthResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PresignedResponse {
    #[serde(rename = "presignedUrl")]
    presigned_url: String,
}

#[handler]
async fn set_auth_service(depot: &mut Depot) {
    let pool = depot.obtain::<DbConnection>().unwrap();

    let auth_repository = AuthRepositoryImpl::new(pool.clone().0);
    let auth_service: AuthServiceImpl = AuthServiceImpl::new(auth_repository);

    depot.inject(auth_service);
}

pub fn get_auth_router(jwt_utils: JwtUtils) -> Router {
    let token_route = Router::with_hoop(jwt_utils.auth_middleware())
        .path("token")
        .post(get_oauth_token);
    let presinged_route = Router::with_hoop(jwt_utils.auth_middleware())
        .path("presigned-url")
        .post(generate_presigned_url);
    let router = Router::new()
        .hoop(set_auth_service)
        .path("auth")
        .post(login_with_social)
        .push(Router::with_hoop(jwt_utils.refresh_token_middleware()).get(refresh_token))
        .push(token_route)
        .push(presinged_route);

    router
}

/// Get Oauth Token
#[endpoint(tags("auth"))]
async fn get_oauth_token(res: &mut Response, data: JsonBody<OauthRequestDto>) {
    let oauth_req: OauthRequestDto = data.0;

    let client = Client::new();
    let resq = client
        .post("https://oauth2.googleapis.com/token")
        .form(&oauth_req)
        .send()
        .await;

    match resq {
        Ok(response) => {
            let oauth_response: Result<OauthResponse, reqwest::Error> = response.json().await;

            match oauth_response {
                Ok(oauth) => res.render(Json(oauth)),
                Err(_) => {
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    res.render("Failed to convert response to OathResponse")
                }
            }
        }
        Err(_) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render("Failed to get OAuth token")
        }
    }
}

/// Get AWS-S3 presigned url
#[endpoint(tags("auth"))]
async fn generate_presigned_url(res: &mut Response, depot: &mut Depot) {
    let env = depot.obtain::<EnvConfig>().unwrap();
    let bucket_name = env.clone().aws.bucket_name;
    let region = env.clone().aws.region;

    let content_type = "image/png";
    // Generate unique file key
    let extension = content_type.split('/').last().unwrap_or("jpeg");
    let key = format!("{}.{}", Uuid::new_v4(), extension);

    // Create AWS S3 client
    let s3_client = get_s3_client(Some(region)).await;

    // Prepare request
    let req = s3_client
        .put_object()
        .bucket(&bucket_name)
        .key(&key)
        .content_type(content_type)
        .acl(ObjectCannedAcl::PublicRead)
        .presigned(PresigningConfig::expires_in(Duration::from_secs(60)).expect(""))
        .await;

    match req {
        Ok(uri) => {
            let presigned_url = PresignedResponse {
                presigned_url: uri.uri().to_string(),
            };

            res.render(Json(presigned_url));
        }
        Err(_) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: "Failed to create presigned url".to_string(),
            }));
        }
    }
}

/// Login
#[endpoint(tags("auth"))]
async fn login_with_social(res: &mut Response, data: JsonBody<LoginDto>, depot: &mut Depot) {
    let auth_service = depot.obtain::<AuthServiceImpl>().unwrap();
    let jwt_utils = depot.obtain::<JwtUtils>().unwrap();

    let auth_response = auth_service
        .login_with_social(data.0, jwt_utils.clone())
        .await;

    match auth_response {
        Ok(response) => {
            res.render(Json(response));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Refresh Token
#[endpoint(tags("auth"))]
async fn refresh_token(res: &mut Response, depot: &mut Depot) {
    let user_id = depot.get::<String>("user_id").unwrap();
    let auth_service = depot.obtain::<AuthServiceImpl>().unwrap();
    let jwt_utils = depot.obtain::<JwtUtils>().unwrap();

    let auth_response = auth_service
        .refresh_token(jwt_utils.clone(), user_id.parse().unwrap())
        .await;

    match auth_response {
        Ok(response) => {
            res.render(Json(response));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}
