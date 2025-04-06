use salvo::{
    oapi::extract::{JsonBody, PathParam, QueryParam},
    prelude::*,
};

use crate::core::{
    dtos::{pagination_dto::PaginationDto, user::update_user_dto::UpdateUserDto},
    types::res::failed_response::FailedResponse,
    utils::jwt_utils::JwtUtils,
};

use super::service::{UserService, UserServiceImpl};

pub fn get_user_router(jwt_utils: JwtUtils) -> Router {
    let router = Router::with_hoop(jwt_utils.auth_middleware())
        .path("users")
        .get(get_user_by_token)
        .put(update_user)
        .push(Router::with_path("search").get(search_user))
        .push(
            Router::with_path("username/{user_name}")
                .get(check_username_exists)
                .put(update_username),
        );

    router
}

/// Fetch user info
#[endpoint(tags("user"))]
async fn get_user_by_token(res: &mut Response, depot: &mut Depot) {
    let user_service = depot.obtain::<UserServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let user = user_service.get_user_by_id(user_id.parse().unwrap()).await;

    match user {
        Ok(user) => {
            res.render(Json(user));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Update user info
#[endpoint(tags("user"))]
async fn update_user(res: &mut Response, data: JsonBody<UpdateUserDto>, depot: &mut Depot) {
    let update_user_dto = data.0;
    let user_service = depot.obtain::<UserServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let user = user_service
        .update_user(user_id.parse().unwrap(), update_user_dto)
        .await;

    match user {
        Ok(user) => {
            res.render(Json(user));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Check username whether it's already exists
#[endpoint(tags("user"))]
async fn check_username_exists(
    res: &mut Response,
    user_name: PathParam<String>,
    depot: &mut Depot,
) {
    let user_service = depot.obtain::<UserServiceImpl>().unwrap();

    let user_name = user_name.into_inner();

    let is_exists = user_service.check_username_exists(&user_name).await;

    res.render(Json(serde_json::json!({ "isRegistered": is_exists })));
}

/// Update username
#[endpoint(tags("user"))]
async fn update_username(res: &mut Response, user_name: PathParam<String>, depot: &mut Depot) {
    let user_service = depot.obtain::<UserServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let user = user_service
        .update_username(user_id.parse().unwrap(), &user_name.0)
        .await;

    match user {
        Ok(user) => {
            res.render(Json(user));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Search user
#[endpoint(tags("user"))]
async fn search_user(_res: &mut Response, _q: QueryParam<String>, _pagination_dto: PaginationDto) {}
