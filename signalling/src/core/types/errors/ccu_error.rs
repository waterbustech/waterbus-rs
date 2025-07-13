use thiserror::Error;

use super::general::GeneralError;

#[derive(Debug, Error)]
pub enum CcuError {
    #[error("CCU not found")]
    NotFoundCcu,

    #[error("Failed to create new ccu")]
    FailedToCreateCcu,

    #[error("Failed to update ccu")]
    FailedToUpdateCcu,

    #[error("Failed to delete ccu with ID {0}")]
    FailedToDeleteCcu(String),

    #[error("General error: {0}")]
    General(#[from] GeneralError),
}
