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
    types::{
        errors::room_error::RoomError,
        responses::{list_room_response::ListRoomResponse, room_response::RoomResponse},
    },
    utils::jwt_utils::JwtUtils,
};

use super::service::{RoomService, RoomServiceImpl};

pub fn get_room_router(jwt_utils: JwtUtils) -> Router {
    let member_router = Router::with_path("/{room_id}/members")
        .post(add_member)
        .delete(delete_member);

    let join_router = Router::with_path("/{room_id}/join").post(join_room);

    let deactivate_router = Router::with_path("/{room_id}/deactivate").post(deactivate_room);

    Router::with_hoop(jwt_utils.auth_middleware())
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
        .push(deactivate_router)
}

/// Retrieves room details using a unique room code.
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn get_room_by_code(
    _res: &mut Response,
    code: PathParam<String>,
    depot: &mut Depot,
) -> Result<RoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();

    let room_code = &code.into_inner();

    let room = room_service.get_room_by_code(room_code).await?;

    Ok(room)
}

/// Allows a user to leave an ongoing room.
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn leave_room(
    _res: &mut Response,
    room_id: PathParam<i32>,
    depot: &mut Depot,
) -> Result<RoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let room_id = room_id.into_inner();

    let room = room_service
        .leave_room(room_id, user_id.parse().unwrap())
        .await?;

    Ok(room)
}

/// Fetches a list of rooms filtered by user
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn get_rooms_by_user(
    _res: &mut Response,
    pagination_dto: PaginationDto,
    depot: &mut Depot,
) -> Result<ListRoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let rooms = room_service
        .get_rooms_by_status(
            RoomStatusEnum::Active as i32,
            user_id.parse().unwrap(),
            pagination_dto.clone(),
        )
        .await?;

    Ok(ListRoomResponse { rooms })
}

/// Fetches rooms that have been deactivated.
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn get_inactive_rooms(
    _res: &mut Response,
    pagination_dto: PaginationDto,
    depot: &mut Depot,
) -> Result<ListRoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let rooms = room_service
        .get_rooms_by_status(
            RoomStatusEnum::Inactive as i32,
            user_id.parse().unwrap(),
            pagination_dto.clone(),
        )
        .await?;

    Ok(ListRoomResponse { rooms })
}

/// Creates a new room
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn create_room(
    _res: &mut Response,
    data: JsonBody<CreateRoomDto>,
    depot: &mut Depot,
) -> Result<RoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let create_room_dto = data.0;

    let room = room_service
        .create_room(create_room_dto, user_id.parse().unwrap())
        .await?;

    Ok(room)
}

/// Updates an existing room
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn update_room(
    _res: &mut Response,
    room_id: PathParam<i32>,
    data: JsonBody<UpdateRoomDto>,
    depot: &mut Depot,
) -> Result<RoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let update_room_dto = data.0;
    let room_id = room_id.into_inner();

    let room = room_service
        .update_room(update_room_dto, room_id, user_id.parse().unwrap())
        .await?;

    Ok(room)
}

/// Adds a new member to a room.
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn add_member(
    _res: &mut Response,
    room_id: PathParam<i32>,
    data: JsonBody<AddMemberDto>,
    depot: &mut Depot,
) -> Result<RoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let host_id = depot.get::<String>("user_id").unwrap();

    let user_id = data.into_inner().user_id;
    let room_id = room_id.into_inner();

    let room = room_service
        .add_member(room_id, host_id.parse().unwrap(), user_id)
        .await?;

    Ok(room)
}

/// Removes a member from a room.
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn delete_member(
    _res: &mut Response,
    room_id: PathParam<i32>,
    data: JsonBody<AddMemberDto>,
    depot: &mut Depot,
) -> Result<RoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let host_id = depot.get::<String>("user_id").unwrap();

    let user_id = data.into_inner().user_id;
    let room_id = room_id.into_inner();

    let room = room_service
        .remove_member(room_id, host_id.parse().unwrap(), user_id)
        .await?;

    Ok(room)
}

/// Joins a room that will be requires a password (for Guess) and not if you're a member
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn join_room(
    _res: &mut Response,
    room_id: PathParam<i32>,
    data: JsonBody<JoinRoomDto>,
    depot: &mut Depot,
) -> Result<RoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let room_id = room_id.into_inner();

    let password = data.into_inner().password;

    let room = room_service
        .join_room(user_id.parse().unwrap(), room_id, password.as_deref())
        .await?;

    Ok(room)
}

/// Deactivates a room, marking it as completed or no longer active.
#[endpoint(tags("room"), status_codes(200, 400, 401, 403, 404, 500))]
async fn deactivate_room(
    _res: &mut Response,
    room_id: PathParam<i32>,
    depot: &mut Depot,
) -> Result<RoomResponse, RoomError> {
    let room_service = depot.obtain::<RoomServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let room_id = room_id.into_inner();

    let room = room_service
        .deactivate_room(room_id, user_id.parse().unwrap())
        .await?;

    Ok(room)
}
