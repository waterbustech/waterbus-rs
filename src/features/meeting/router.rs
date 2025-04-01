use salvo::prelude::*;

use crate::core::utils::jwt_utils::JwtUtils;

pub fn get_meeting_router(jwt_utils: JwtUtils) -> Router {
    let conversation_router = Router::with_path("conversations")
        .get(get_meetings_by_status)
        .push(Router::with_path("archived").get(get_archived_meetings));

    let member_router = Router::with_path("members")
        .post(add_member)
        .delete(delete_member)
        .push(Router::with_path("accept").post(accept_invitation));

    let join_router = Router::with_path("join")
        .post(join_meeting_without_password)
        .push(Router::with_path("password").post(join_meeting_with_password));

    let archived_router = Router::with_path("archived").post(archived_meeting);

    let record_router = Router::with_path("records")
        .get(get_records)
        .post(start_records)
        .delete(stop_records);

    let router = Router::with_hoop(jwt_utils.auth_middleware())
        .path("meetings")
        .get(get_meeting_by_code)
        .delete(leave_meeting)
        .post(create_meeting)
        .put(update_meeting)
        .push(conversation_router)
        .push(member_router)
        .push(join_router)
        .push(archived_router)
        .push(record_router);

    router
}

/// Retrieves meeting details using a unique meeting code.
#[endpoint(tags("meeting"))]
async fn get_meeting_by_code(res: &mut Response) {}

/// Allows a user to leave an ongoing meeting.
#[endpoint(tags("meeting"))]
async fn leave_meeting(res: &mut Response) {}

/// Fetches a list of meetings filtered by their status (e.g., active, scheduled).
#[endpoint(tags("meeting"))]
async fn get_meetings_by_status(res: &mut Response) {}

/// Retrieves meetings that have been archived.
#[endpoint(tags("meeting"))]
async fn get_archived_meetings(res: &mut Response) {}

/// Creates a new meeting with specified parameters.
#[endpoint(tags("meeting"))]
async fn create_meeting(res: &mut Response) {}

/// Updates an existing meeting’s details.
#[endpoint(tags("meeting"))]
async fn update_meeting(res: &mut Response) {}

/// Adds a new member to a meeting.
#[endpoint(tags("meeting"))]
async fn add_member(res: &mut Response) {}

/// Removes a member from a meeting.
#[endpoint(tags("meeting"))]
async fn delete_member(res: &mut Response) {}

/// Accepts an invitation to join a meeting.
#[endpoint(tags("meeting"))]
async fn accept_invitation(res: &mut Response) {}

/// Joins a meeting that requires a password. (for Guess)
#[endpoint(tags("meeting"))]
async fn join_meeting_with_password(res: &mut Response) {}

/// Joins a meeting that does not require a password. (for Member)
#[endpoint(tags("meeting"))]
async fn join_meeting_without_password(res: &mut Response) {}

/// Archives a meeting, marking it as completed or no longer active.
#[endpoint(tags("meeting"))]
async fn archived_meeting(res: &mut Response) {}

/// Retrieves a list of meeting recordings.
#[endpoint(tags("meeting"))]
async fn get_records(res: &mut Response) {}

/// Starts recording the current meeting session.
#[endpoint(tags("meeting"))]
async fn start_records(res: &mut Response) {}

/// Stops an ongoing recording of the meeting.
#[endpoint(tags("meeting"))]
async fn stop_records(res: &mut Response) {}
