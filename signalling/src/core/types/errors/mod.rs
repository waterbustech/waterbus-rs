use salvo::oapi::ToSchema;
use serde::Serialize;

pub mod auth_error;
pub mod ccu_error;
pub mod chat_error;
pub mod general;
pub mod room_error;
pub mod user_error;

#[derive(Debug, ToSchema, Serialize)]
#[salvo(schema(example = json!({"message": ""})))]
struct NotFoundError {
    message: String,
}

#[derive(Debug, ToSchema, Serialize)]
#[salvo(schema(example = json!({"message": ""})))]
struct BadRequestError {
    message: String,
}

#[derive(Debug, ToSchema, Serialize)]
#[salvo(schema(example = json!({"message": ""})))]
struct InternalError {
    message: String,
}
