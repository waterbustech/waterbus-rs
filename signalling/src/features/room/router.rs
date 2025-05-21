use salvo::{
    oapi::extract::{JsonBody, PathParam},
    prelude::*,
};

use crate::core::{
    dtos::{
        common::pagination_dto::PaginationDto,
        room::{
            add_member_dto::AddMemberDto, create_room_dto::CreateRoomDto,
            join_room_dto::JoinRoomDto, update_room_dto::UpdateRoomDto,
        },
    },
    entities::models::RoomStatusEnum,
    types::res::failed_response::FailedResponse,
    utils::jwt_utils::JwtUtils,
};

use super::service::{RoomService, RoomServiceImpl};

pub fn get_room_router(jwt_utils: JwtUtils) -> Router {
    let member_router = Router::with_path("/{room_id}/members")
        .post(add_member)
        .delete(delete_member);

    let join_router = Router::with_path("/{room_id}/join").post(join_room);

    let deactivate_router = Router::with_path("/{room_id}/deactivate").post(deactivate_room);

    let router = Router::with_hoop(jwt_utils.auth_middleware())
        .path("rooms")
        .post(create_room)
        .get(get_rooms_by_user)
        .push(Router::with_path("inactive").get(get_inactive_rooms))
        .push(Router::with_path("/{code}").get(get_room_by_code))
        .push(
            Router::with_path("/{room_id}")
                .put(update_room)
                .delete(leave_room),
        )
        .push(member_router)
        .push(join_router)
        .push(deactivate_router);

    router
}

/// Retrieves room details using a unique room code.
#[endpoint(tags("room"))]
async fn get_room_by_code(res: &mut Response, code: PathParam<String>, depot: &mut Depot) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();

    let room_code = &code.into_inner();

    let room = room_service.get_room_by_code(room_code).await;

    match room {
        Ok(room) => {
            res.render(Json(room));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Allows a user to leave an ongoing room.
#[endpoint(tags("room"))]
async fn leave_room(res: &mut Response, room_id: PathParam<i32>, depot: &mut Depot) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let room_id = room_id.into_inner();

    let room = room_service
        .leave_room(room_id, user_id.parse().unwrap())
        .await;

    match room {
        Ok(room) => {
            res.render(Json(room));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Fetches a list of rooms filtered by user
#[endpoint(tags("room"))]
async fn get_rooms_by_user(res: &mut Response, pagination_dto: PaginationDto, depot: &mut Depot) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let rooms = room_service
        .get_rooms_by_status(
            RoomStatusEnum::Active as i32,
            user_id.parse().unwrap(),
            pagination_dto.clone(),
        )
        .await;

    match rooms {
        Ok(rooms) => {
            res.render(Json(rooms));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Fetches rooms that have been deactivated.
#[endpoint(tags("room"))]
async fn get_inactive_rooms(res: &mut Response, pagination_dto: PaginationDto, depot: &mut Depot) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let rooms = room_service
        .get_rooms_by_status(
            RoomStatusEnum::Inactive as i32,
            user_id.parse().unwrap(),
            pagination_dto.clone(),
        )
        .await;

    match rooms {
        Ok(rooms) => {
            res.render(Json(rooms));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Creates a new room
#[endpoint(tags("room"))]
async fn create_room(res: &mut Response, data: JsonBody<CreateRoomDto>, depot: &mut Depot) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let create_room_dto = data.0;

    let room = room_service
        .create_room(create_room_dto, user_id.parse().unwrap())
        .await;

    match room {
        Ok(room) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(room));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Updates an existing room
#[endpoint(tags("room"))]
async fn update_room(
    res: &mut Response,
    room_id: PathParam<i32>,
    data: JsonBody<UpdateRoomDto>,
    depot: &mut Depot,
) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let update_room_dto = data.0;
    let room_id = room_id.into_inner();

    let room = room_service
        .update_room(update_room_dto, room_id, user_id.parse().unwrap())
        .await;

    match room {
        Ok(room) => {
            res.render(Json(room));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Adds a new member to a room.
#[endpoint(tags("room"))]
async fn add_member(
    res: &mut Response,
    room_id: PathParam<i32>,
    data: JsonBody<AddMemberDto>,
    depot: &mut Depot,
) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let host_id = depot.get::<String>("user_id").unwrap();

    let user_id = data.into_inner().user_id;
    let room_id = room_id.into_inner();

    let room = room_service
        .add_member(room_id, host_id.parse().unwrap(), user_id)
        .await;

    match room {
        Ok(room) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(room));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Removes a member from a room.
#[endpoint(tags("room"))]
async fn delete_member(
    res: &mut Response,
    room_id: PathParam<i32>,
    data: JsonBody<AddMemberDto>,
    depot: &mut Depot,
) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let host_id = depot.get::<String>("user_id").unwrap();

    let user_id = data.into_inner().user_id;
    let room_id = room_id.into_inner();

    let room = room_service
        .remove_member(room_id, host_id.parse().unwrap(), user_id)
        .await;

    match room {
        Ok(room) => {
            res.render(Json(room));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Joins a room that will be requires a password (for Guess) and not if you're a member
#[endpoint(tags("room"))]
async fn join_room(
    res: &mut Response,
    room_id: PathParam<i32>,
    data: JsonBody<JoinRoomDto>,
    depot: &mut Depot,
) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let room_id = room_id.into_inner();

    let password = data.into_inner().password;

    let room = room_service
        .join_room(user_id.parse().unwrap(), room_id, password.as_deref())
        .await;

    match room {
        Ok(room) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(room));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Deactivates a room, marking it as completed or no longer active.
#[endpoint(tags("room"))]
async fn deactivate_room(res: &mut Response, room_id: PathParam<i32>, depot: &mut Depot) {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let room_id = room_id.into_inner();

    let room = room_service
        .deactivate_room(room_id, user_id.parse().unwrap())
        .await;

    match room {
        Ok(room) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(room));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}
