use std::time::Duration;

use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::ObjectCannedAcl;
use salvo::oapi::extract::JsonBody;
use salvo::prelude::*;
use salvo::{Response, Router, oapi::endpoint};
use uuid::Uuid;

use crate::core::dtos::auth::create_token_dto::CreateTokenDto;
use crate::core::types::errors::auth_error::AuthError;
use crate::core::types::responses::auth_response::AuthResponse;
use crate::core::types::responses::failed_response::FailedResponse;
use crate::core::types::responses::presigned_url_response::PresignedResponse;
use crate::core::utils::aws_utils::get_storage_object_client;
use crate::core::utils::jwt_utils::JwtUtils;

use super::service::{AuthService, AuthServiceImpl};

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

/// Get presigned url
#[endpoint(tags("auth"), status_codes(201, 400))]
async fn generate_presigned_url(_res: &mut Response) -> Result<PresignedResponse, FailedResponse> {
    let content_type = "image/png";
    // Generate unique file key
    let extension = content_type.split('/').last().unwrap_or("jpeg");
    let key = format!("{}.{}", Uuid::new_v4(), extension);

    // Create storage object client
    let (object_client, bucket_name) = get_storage_object_client().await;

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

            return Ok(presigned_url);
        }
        Err(_) => {
            return Err(FailedResponse {
                message: "Failed to create presigned url".to_string(),
            });
        }
    }
}

/// Create token
#[endpoint(tags("auth"), status_codes(201, 400, 401, 500))]
async fn create_token(
    _res: &mut Response,
    data: JsonBody<CreateTokenDto>,
    depot: &mut Depot,
) -> Result<AuthResponse, AuthError> {
    let auth_service = depot.obtain::<AuthServiceImpl>().unwrap();
    let jwt_utils = depot.obtain::<JwtUtils>().unwrap();

    let auth_response = auth_service
        .login_with_social(data.0, jwt_utils.clone())
        .await?;

    Ok(auth_response)
}

/// Renew Token
#[endpoint(tags("auth"), status_codes(200, 400, 404, 500))]
async fn refresh_token(_res: &mut Response, depot: &mut Depot) -> Result<AuthResponse, AuthError> {
    let user_id = depot.get::<String>("user_id").unwrap();
    let auth_service = depot.obtain::<AuthServiceImpl>().unwrap();
    let jwt_utils = depot.obtain::<JwtUtils>().unwrap();

    let auth_response = auth_service
        .refresh_token(jwt_utils.clone(), user_id.parse().unwrap())
        .await?;

    Ok(auth_response)
}
