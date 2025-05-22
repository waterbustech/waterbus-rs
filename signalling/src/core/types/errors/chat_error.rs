use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;
use thiserror::Error;

use super::general::GeneralError;

use super::{BadRequestError, InternalError, NotFoundError};

#[derive(Debug, Error, ToSchema, Serialize)]
pub enum ChatError {
    #[error("Message with ID {0} not found")]
    MessageNotFound(i32),

    #[error("Member with ID {0} not found")]
    MemberNotFound(i32),

    #[error("Forbiden: {0}")]
    Forbidden(String),

    #[error("Conversation with ID {0} not found")]
    ConversationNotFound(i32),

    #[error("An unexpected error occurred in channel: {0}")]
    UnexpectedError(String),

    #[error("General error: {0}")]
    General(#[from] GeneralError),
}

#[async_trait]
impl Writer for ChatError {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        let status = match self {
            ChatError::MemberNotFound(_)
            | ChatError::ConversationNotFound(_)
            | ChatError::MessageNotFound(_) => StatusCode::NOT_FOUND,
            ChatError::Forbidden(_) => StatusCode::FORBIDDEN,
            ChatError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ChatError::General(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        res.status_code(status);
        res.render(Json(serde_json::json!({ "message": self.to_string() })));
    }
}

impl EndpointOutRegister for ChatError {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::NOT_FOUND.as_str(),
            oapi::Response::new("Conversation not found")
                .add_content("application/json", NotFoundError::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::FORBIDDEN.as_str(),
            oapi::Response::new("Forbiden:")
                .add_content("application/json", BadRequestError::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::INTERNAL_SERVER_ERROR.as_str(),
            oapi::Response::new("Unexpected or general error")
                .add_content("application/json", InternalError::to_schema(components)),
        );
    }
}
