use salvo::{
    oapi::extract::{JsonBody, PathParam, QueryParam},
    prelude::*,
};

use crate::core::{
    dtos::{
        pagination_dto::{self, PaginationDto},
        user::update_user_dto::UpdateUserDto,
    },
    utils::jwt_utils::JwtUtils,
};

pub fn get_user_router(jwt_utils: JwtUtils) -> Router {
    let router = Router::with_hoop(jwt_utils.auth_middleware())
        .path("users")
        .get(get_user_by_token)
        .put(update_user)
        .push(Router::with_path("search").get(search_user))
        .push(
            Router::with_path("username")
                .get(check_username_exists)
                .put(update_username),
        );

    router
}

/// Search user
#[endpoint(tags("user"))]
async fn search_user(
    res: &mut Response,
    q: QueryParam<String>,
    pagination_dto: QueryParam<PaginationDto>,
) {
}

/// Fetch user info
#[endpoint(tags("user"))]
async fn get_user_by_token(res: &mut Response) {}

/// Update user info
#[endpoint(tags("user"))]
async fn update_user(res: &mut Response, data: JsonBody<UpdateUserDto>) {}

/// Check username whether it's already exists
#[endpoint(tags("user"))]
async fn check_username_exists(res: &mut Response, user_name: PathParam<String>) {}

/// Update username
#[endpoint(tags("user"))]
async fn update_username(res: &mut Response, user_name: PathParam<String>) {}
