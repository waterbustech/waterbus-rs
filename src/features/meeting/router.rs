use salvo::{
    oapi::extract::{JsonBody, PathParam},
    prelude::*,
};

use crate::core::{
    dtos::{
        meeting::{
            add_member_dto::AddMemberDto, create_meeting_dto::CreateMeetingDto,
            join_meeting_dto::JoinMeetingDto, update_meeting_dto::UpdateMeetingDto,
        },
        pagination_dto::PaginationDto,
    },
    entities::models::{MeetingsStatusEnum, MembersStatusEnum},
    types::res::failed_response::FailedResponse,
    utils::jwt_utils::JwtUtils,
};

use super::service::{MeetingService, MeetingServiceImpl};

pub fn get_meeting_router(jwt_utils: JwtUtils) -> Router {
    let conversation_router = Router::with_path("conversations")
        .push(Router::with_path("/{status}").get(get_meetings_by_status))
        .push(Router::with_path("archived").get(get_archived_meetings));

    let member_router = Router::with_path("members")
        .push(
            Router::with_path("/{code}")
                .post(add_member)
                .delete(delete_member),
        )
        .push(Router::with_path("accept/{meeting_id}").post(accept_invitation));

    let join_router = Router::with_path("join")
        .push(Router::with_path("/{code}").post(join_meeting_without_password))
        .push(Router::with_path("password/{code}").post(join_meeting_with_password));

    let archived_router = Router::with_path("archived/{code}").post(archived_meeting);

    let record_router = Router::with_path("records").get(get_records).push(
        Router::with_path("/{code}")
            .post(start_records)
            .delete(stop_records),
    );

    let router = Router::with_hoop(jwt_utils.auth_middleware())
        .path("meetings")
        .post(create_meeting)
        .put(update_meeting)
        .push(
            Router::with_path("/{code}")
                .get(get_meeting_by_code)
                .delete(leave_meeting),
        )
        .push(conversation_router)
        .push(member_router)
        .push(join_router)
        .push(archived_router)
        .push(record_router);

    router
}

/// Retrieves meeting details using a unique meeting code.
#[endpoint(tags("meeting"))]
async fn get_meeting_by_code(res: &mut Response, code: PathParam<i32>, depot: &mut Depot) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();

    let meeting_code = code.into_inner();

    let meeting = meeting_service.get_meeting_by_code(meeting_code).await;

    match meeting {
        Ok(meeting) => {
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Allows a user to leave an ongoing meeting.
#[endpoint(tags("meeting"))]
async fn leave_meeting(res: &mut Response, code: PathParam<i32>, depot: &mut Depot) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let meeting_code = code.into_inner();

    let meeting = meeting_service
        .leave_meeting(meeting_code, user_id.parse().unwrap())
        .await;

    match meeting {
        Ok(meeting) => {
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Fetches a list of meetings filtered by their status (e.g., active, scheduled).
#[endpoint(tags("meeting"))]
async fn get_meetings_by_status(
    res: &mut Response,
    status: PathParam<i32>,
    pagination_dto: PaginationDto,
    depot: &mut Depot,
) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let status = status.into_inner();

    let meetings = meeting_service
        .get_meetings_by_status(
            status,
            MeetingsStatusEnum::Active as i32,
            user_id.parse().unwrap(),
            pagination_dto.clone(),
        )
        .await;

    match meetings {
        Ok(meetings) => {
            res.render(Json(meetings));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Retrieves meetings that have been archived.
#[endpoint(tags("meeting"))]
async fn get_archived_meetings(
    res: &mut Response,
    pagination_dto: PaginationDto,
    depot: &mut Depot,
) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let meetings = meeting_service
        .get_meetings_by_status(
            MembersStatusEnum::Joined as i32,
            MeetingsStatusEnum::Archived as i32,
            user_id.parse().unwrap(),
            pagination_dto.clone(),
        )
        .await;

    match meetings {
        Ok(meetings) => {
            res.render(Json(meetings));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Creates a new meeting with specified parameters.
#[endpoint(tags("meeting"))]
async fn create_meeting(res: &mut Response, data: JsonBody<CreateMeetingDto>, depot: &mut Depot) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();
    let create_meeting_dto = data.0;

    let meeting = meeting_service
        .create_meeting(create_meeting_dto, user_id.parse().unwrap())
        .await;

    match meeting {
        Ok(meeting) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Updates an existing meeting’s details.
#[endpoint(tags("meeting"))]
async fn update_meeting(res: &mut Response, data: JsonBody<UpdateMeetingDto>, depot: &mut Depot) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let update_meeting_dto = data.0;

    let meeting = meeting_service
        .update_meeting(update_meeting_dto, user_id.parse().unwrap())
        .await;
    match meeting {
        Ok(meeting) => {
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Adds a new member to a meeting.
#[endpoint(tags("meeting"))]
async fn add_member(
    res: &mut Response,
    code: PathParam<i32>,
    data: JsonBody<AddMemberDto>,
    depot: &mut Depot,
) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let host_id = depot.get::<String>("user_id").unwrap();

    let user_id = data.into_inner().user_id;
    let meeting_code = code.into_inner();

    let meeting = meeting_service
        .add_member(meeting_code, host_id.parse().unwrap(), user_id)
        .await;

    match meeting {
        Ok(meeting) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Removes a member from a meeting.
#[endpoint(tags("meeting"))]
async fn delete_member(
    res: &mut Response,
    code: PathParam<i32>,
    data: JsonBody<AddMemberDto>,
    depot: &mut Depot,
) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let host_id = depot.get::<String>("user_id").unwrap();

    let user_id = data.into_inner().user_id;
    let meeting_code = code.into_inner();

    let meeting = meeting_service
        .remove_member(meeting_code, host_id.parse().unwrap(), user_id)
        .await;

    match meeting {
        Ok(meeting) => {
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Accepts an invitation to join a meeting.
#[endpoint(tags("meeting"))]
async fn accept_invitation(res: &mut Response, meeting_id: PathParam<i32>, depot: &mut Depot) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let meeting_id = meeting_id.into_inner();

    let meeting = meeting_service
        .accept_invitation(meeting_id, user_id.parse().unwrap())
        .await;

    match meeting {
        Ok(meeting) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Joins a meeting that requires a password. (for Guess)
#[endpoint(tags("meeting"))]
async fn join_meeting_with_password(
    res: &mut Response,
    code: PathParam<i32>,
    data: JsonBody<JoinMeetingDto>,
    depot: &mut Depot,
) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let meeting_code = code.into_inner();
    let password = data.into_inner().password;

    let meeting = meeting_service
        .join_with_password(user_id.parse().unwrap(), meeting_code, &password)
        .await;

    match meeting {
        Ok(meeting) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Joins a meeting that does not require a password. (for Member)
#[endpoint(tags("meeting"))]
async fn join_meeting_without_password(
    res: &mut Response,
    code: PathParam<i32>,
    depot: &mut Depot,
) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let meeting_code = code.into_inner();

    let meeting = meeting_service
        .join_meeting_without_password(user_id.parse().unwrap(), meeting_code)
        .await;

    match meeting {
        Ok(meeting) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Archives a meeting, marking it as completed or no longer active.
#[endpoint(tags("meeting"))]
async fn archived_meeting(res: &mut Response, code: PathParam<i32>, depot: &mut Depot) {
    let meeting_service = depot.obtain::<MeetingServiceImpl>().unwrap();
    let user_id = depot.get::<String>("user_id").unwrap();

    let meeting_code = code.into_inner();

    let meeting = meeting_service
        .archived_meeting(meeting_code, user_id.parse().unwrap())
        .await;

    match meeting {
        Ok(meeting) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(meeting));
        }
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(FailedResponse {
                message: err.to_string(),
            }));
        }
    }
}

/// Retrieves a list of meeting recordings.
#[endpoint(tags("meeting"))]
async fn get_records(_res: &mut Response, _pagination_dto: PaginationDto) {}

/// Starts recording the current meeting session.
#[endpoint(tags("meeting"))]
async fn start_records(_res: &mut Response, _code: PathParam<i32>) {}

/// Stops an ongoing recording of the meeting.
#[endpoint(tags("meeting"))]
async fn stop_records(_res: &mut Response, _code: PathParam<i32>) {}
