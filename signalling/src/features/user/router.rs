use salvo::{
    oapi::extract::{JsonBody, PathParam},
    prelude::*,
};

use crate::core::{
    dtos::user::update_user_dto::UpdateUserDto,
    entities::models::User,
    types::{
        errors::user_error::UserError, responses::check_username_response::CheckUsernameResponse,
    },
    utils::jwt_utils::JwtUtils,
};

use super::service::{UserService, UserServiceImpl};

pub fn get_user_router(jwt_utils: JwtUtils) -> Router {
    Router::with_hoop(jwt_utils.auth_middleware())
        .path("users")
        .get(get_user_by_token)
        .put(update_user)
        .push(
            Router::with_path("username/{user_name}")
                .get(check_username_exists)
                .put(update_username),
        )
}

/// Fetch user info
#[endpoint(tags("user"), status_codes(200, 400, 404, 500))]
async fn get_user_by_token(_res: &mut Response, depot: &mut Depot) -> Result<User, UserError> {
    let user_service = depot.obtain::<UserServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let user = user_service
        .get_user_by_id(user_id.parse().unwrap())
        .await?;

    Ok(user)
}

/// Update user info
#[endpoint(tags("user"), status_codes(200, 400, 404, 500))]
async fn update_user(
    _res: &mut Response,
    data: JsonBody<UpdateUserDto>,
    depot: &mut Depot,
) -> Result<User, UserError> {
    let update_user_dto = data.0;
    let user_service = depot.obtain::<UserServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let user = user_service
        .update_user(user_id.parse().unwrap(), update_user_dto)
        .await?;

    Ok(user)
}

/// Check username whether it's already exists
#[endpoint(tags("user"), status_codes(200))]
async fn check_username_exists(
    _res: &mut Response,
    user_name: PathParam<String>,
    depot: &mut Depot,
) -> CheckUsernameResponse {
    let user_service = depot.obtain::<UserServiceImpl>().unwrap();

    let user_name = user_name.into_inner();

    let is_exists = user_service.check_username_exists(&user_name).await;

    CheckUsernameResponse {
        is_registered: is_exists,
    }
}

/// Update username
#[endpoint(tags("user"), status_codes(200, 400, 404, 500))]
async fn update_username(
    _res: &mut Response,
    user_name: PathParam<String>,
    depot: &mut Depot,
) -> Result<User, UserError> {
    let user_service = depot.obtain::<UserServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let user = user_service
        .update_username(user_id.parse().unwrap(), &user_name.0)
        .await?;

    Ok(user)
}
