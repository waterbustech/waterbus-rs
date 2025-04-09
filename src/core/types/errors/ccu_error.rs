use thiserror::Error;

use super::general::GeneralError;

#[derive(Debug, Error)]
pub enum CcuError {
    #[error("CCU with ID {0} not found")]
    NotFoundCcu(i32),

    #[error("Failed to create new ccu")]
    FailedToCreateCcu,

    #[error("Failed to update ccu")]
    FailedToUpdateCcu,

    #[error("Failed to delete ccu with ID {0}")]
    FailedToDeleteCcu(i32),

    #[error("General error: {0}")]
    General(#[from] GeneralError),
}
