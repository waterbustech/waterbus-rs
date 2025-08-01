use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;
use thiserror::Error;

use super::general::GeneralError;
use super::{BadRequestError, InternalError, NotFoundError};

#[derive(Debug, Error, Serialize, ToSchema, Clone)]
pub enum UserError {
    #[error("User with ID {0} not found")]
    UserNotFound(i32),

    #[error("User with username {0} not found")]
    UserNameNotFound(String),

    #[error("User with ID {0} is already exists")]
    UserExists(i32),

    #[error("An unexpected error occurred in channel: {0}")]
    UnexpectedError(String),

    #[error("General error: {0}")]
    General(#[from] GeneralError),
}

#[async_trait]
impl Writer for UserError {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        let status = match self {
            UserError::UserNotFound(_) | UserError::UserNameNotFound(_) => StatusCode::NOT_FOUND,
            UserError::UserExists(_) => StatusCode::BAD_REQUEST,
            UserError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UserError::General(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        res.status_code(status);
        res.render(Json(serde_json::json!({ "message": self.to_string() })));
    }
}

impl EndpointOutRegister for UserError {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::NOT_FOUND.as_str(),
            oapi::Response::new("User not found")
                .add_content("application/json", NotFoundError::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::BAD_REQUEST.as_str(),
            oapi::Response::new("User already exists or bad request")
                .add_content("application/json", BadRequestError::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::INTERNAL_SERVER_ERROR.as_str(),
            oapi::Response::new("Unexpected or general error")
                .add_content("application/json", InternalError::to_schema(components)),
        );
    }
}
