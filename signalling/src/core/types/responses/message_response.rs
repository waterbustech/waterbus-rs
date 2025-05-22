use salvo::http::{Method, StatusCode};
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;

use crate::core::entities::models::{Message, Room, User};

#[derive(Debug, Serialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageResponse {
    #[serde(flatten)]
    pub message: Message,
    pub created_by: Option<User>,
    pub room: Option<Room>,
}

#[async_trait]
impl Writer for MessageResponse {
    async fn write(self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        if req.method() == Method::POST {
            res.status_code(StatusCode::CREATED);
            res.render(Json(self));
        } else {
            res.status_code(StatusCode::OK);
            res.render(Json(self));
        }
    }
}

impl EndpointOutRegister for MessageResponse {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::OK.as_str(),
            oapi::Response::new("OK")
                .add_content("application/json", MessageResponse::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::CREATED.as_str(),
            oapi::Response::new("Created")
                .add_content("application/json", MessageResponse::to_schema(components)),
        );
    }
}
