use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;
use thiserror::Error;

use super::general::GeneralError;
use super::{BadRequestError, InternalError, NotFoundError};

#[derive(Debug, Error, ToSchema, Serialize, Clone)]
pub enum AuthError {
    #[error("Invalid API Key")]
    InvalidAPIKey,

    #[error("Invalid token")]
    InvalidToken,

    #[error("User with ID {0} is already exists")]
    UserExists(i32),

    #[error("User with ID {0} not found")]
    UserNotFound(i32),

    #[error("An unexpected error occurred in auth: {0}")]
    UnexpectedError(String),

    #[error("Failed to contact Cloudflare: {0}")]
    CloudflareError(String),

    #[error("General error: {0}")]
    General(#[from] GeneralError),
}

#[async_trait]
impl Writer for AuthError {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        let status = match self {
            AuthError::UserNotFound(_) => StatusCode::NOT_FOUND,
            AuthError::UserExists(_) => StatusCode::BAD_REQUEST,
            AuthError::InvalidAPIKey | AuthError::InvalidToken => StatusCode::UNAUTHORIZED,
            AuthError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AuthError::General(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AuthError::CloudflareError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        res.status_code(status);
        res.render(Json(serde_json::json!({ "message": self.to_string() })));
    }
}

impl EndpointOutRegister for AuthError {
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
