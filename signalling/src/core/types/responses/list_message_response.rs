use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;

use super::message_response::MessageResponse;

#[derive(Debug, Serialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListMessageResponse {
    pub messages: Vec<MessageResponse>,
}

#[async_trait]
impl Writer for ListMessageResponse {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.status_code(StatusCode::OK);
        res.render(Json(self));
    }
}

impl EndpointOutRegister for ListMessageResponse {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::OK.as_str(),
            oapi::Response::new("OK").add_content(
                "application/json",
                ListMessageResponse::to_schema(components),
            ),
        );
    }
}
