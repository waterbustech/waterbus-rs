use salvo::oapi::ToSchema;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, ToSchema, Serialize, Clone, PartialEq)]
pub enum GeneralError {
    #[error("Database connection failed")]
    DbConnectionError,
}
