use thiserror::Error;

use super::general::GeneralError;

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("Message with ID {0} not found")]
    MessageNotFound(i32),

    #[error("Message with ID {0} is already exists")]
    MessageExists(i32),

    #[error("General error: {0}")]
    General(#[from] GeneralError),
}
