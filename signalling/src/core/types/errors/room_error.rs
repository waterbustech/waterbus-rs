use thiserror::Error;

use super::general::GeneralError;

#[derive(Debug, Error)]
pub enum RoomError {
    #[error("Room with ID {0} not found")]
    RoomNotFound(i32),

    #[error("Room with Code {0} not found")]
    RoomCodeNotFound(String),

    #[error("Room with ID {0} is already exists")]
    RoomExists(i32),

    #[error("Owner can not leave the room")]
    OwnerCannotLeaveRoom,

    #[error("Only the host has permission")]
    YouDontHavePermissions,

    #[error("Password is not correct")]
    PasswordIncorrect,

    #[error("An unexpected error occurred in channel: {0}")]
    UnexpectedError(String),

    #[error("General error: {0}")]
    General(#[from] GeneralError),
}
