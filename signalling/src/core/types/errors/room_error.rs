use super::general::GeneralError;
use super::{BadRequestError, InternalError, NotFoundError};
use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, ToSchema, Serialize, Clone, PartialEq)]
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
    #[error("Room is full")]
    RoomIsFull,
    #[error("General error: {0}")]
    General(#[from] GeneralError),
}

#[async_trait]
impl Writer for RoomError {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        let status = match self {
            RoomError::RoomNotFound(_) | RoomError::RoomCodeNotFound(_) => StatusCode::NOT_FOUND,
            RoomError::RoomExists(_) => StatusCode::BAD_REQUEST,
            RoomError::YouDontHavePermissions | RoomError::OwnerCannotLeaveRoom => {
                StatusCode::FORBIDDEN
            }
            RoomError::PasswordIncorrect => StatusCode::UNAUTHORIZED,
            RoomError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RoomError::General(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RoomError::RoomIsFull => StatusCode::BAD_REQUEST,
        };
        res.status_code(status);
        res.render(Json(serde_json::json!({ "message": self.to_string() })));
    }
}

impl EndpointOutRegister for RoomError {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::NOT_FOUND.as_str(),
            oapi::Response::new("Room not found")
                .add_content("application/json", NotFoundError::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::BAD_REQUEST.as_str(),
            oapi::Response::new("Room already exists or bad request")
                .add_content("application/json", BadRequestError::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::FORBIDDEN.as_str(),
            oapi::Response::new("Insufficient permissions or forbidden action")
                .add_content("application/json", BadRequestError::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::UNAUTHORIZED.as_str(),
            oapi::Response::new("Incorrect password or unauthorized")
                .add_content("application/json", BadRequestError::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::INTERNAL_SERVER_ERROR.as_str(),
            oapi::Response::new("Unexpected or general error")
                .add_content("application/json", InternalError::to_schema(components)),
        );
    }
}
