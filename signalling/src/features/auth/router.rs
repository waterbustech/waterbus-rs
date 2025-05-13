use std::time::Duration;

use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::ObjectCannedAcl;
use salvo::oapi::extract::JsonBody;
use salvo::prelude::*;
use salvo::{Response, Router, oapi::endpoint};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::dtos::auth::login_dto::LoginDto;
use crate::core::env::app_env::AppEnv;
use crate::core::types::res::failed_response::FailedResponse;
use crate::core::utils::aws_utils::get_storage_object_client;
use crate::core::utils::jwt_utils::JwtUtils;

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

pub fn get_auth_router(jwt_utils: JwtUtils) -> Router {
    let presinged_route = Router::with_hoop(jwt_utils.auth_middleware())
        .path("presigned-url")
        .post(generate_presigned_url);
    let router = Router::new()
        .path("auth")
        .post(create_token)
        .push(Router::with_hoop(jwt_utils.refresh_token_middleware()).get(refresh_token))
        .push(presinged_route);

    router
}

/// Get AWS-S3 or Cloudflare-R2 presigned url
#[endpoint(tags("auth"))]
async fn generate_presigned_url(res: &mut Response, depot: &mut Depot) {
    let env = depot.obtain::<AppEnv>().unwrap();
    let bucket_name = env.clone().aws.bucket_name;

    let content_type = "image/png";
    // Generate unique file key
    let extension = content_type.split('/').last().unwrap_or("jpeg");
    let key = format!("{}.{}", Uuid::new_v4(), extension);

    // Create storage object client
    let object_client = get_storage_object_client().await;

    // Prepare request
    let req = object_client
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

            res.status_code(StatusCode::CREATED);
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

/// Create token
#[endpoint(tags("auth"))]
async fn create_token(res: &mut Response, data: JsonBody<LoginDto>, depot: &mut Depot) {
    let auth_service = depot.obtain::<AuthServiceImpl>().unwrap();
    let jwt_utils = depot.obtain::<JwtUtils>().unwrap();

    let auth_response = auth_service
        .login_with_social(data.0, jwt_utils.clone())
        .await;

    match auth_response {
        Ok(response) => {
            res.status_code(StatusCode::CREATED);
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

/// Renew Token
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
