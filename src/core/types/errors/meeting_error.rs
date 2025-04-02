use thiserror::Error;

use super::general::GeneralError;

#[derive(Debug, Error)]
pub enum MeetingError {
    #[error("Meeting with ID {0} not found")]
    MeetingNotFound(i32),

    #[error("Meeting with ID {0} is already exists")]
    MeetingExists(i32),

    #[error("Host can not leave the meeting")]
    HostCannotLeaveMeeting,

    #[error("Only the host has permission")]
    YouDontHavePermissions,

    #[error("An unexpected error occurred in channel: {0}")]
    UnexpectedError(String),

    #[error("General error: {0}")]
    General(#[from] GeneralError),
}
