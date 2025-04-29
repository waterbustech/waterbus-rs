use thiserror::Error;

#[derive(Debug, Error)]
pub enum GeneralError {
    #[error("Database connection failed")]
    DbConnectionError,
}
