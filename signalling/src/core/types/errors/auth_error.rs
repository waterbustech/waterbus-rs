use thiserror::Error;

use super::general::GeneralError;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("User with ID {0} not found")]
    UserNotFound(i32),

    #[error("User with ID {0} is already exists")]
    UserExists(i32),

    #[error("An unexpected error occurred in channel: {0}")]
    UnexpectedError(String),

    #[error("General error: {0}")]
    General(#[from] GeneralError),
}
