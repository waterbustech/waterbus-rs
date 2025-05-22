use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize, ToSchema)]
pub struct FailedResponse {
    pub message: String,
}

#[async_trait]
impl Writer for FailedResponse {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(self));
    }
}

impl EndpointOutRegister for FailedResponse {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::BAD_REQUEST.as_str(),
            oapi::Response::new("BAD REQUEST")
                .add_content("application/json", FailedResponse::to_schema(components)),
        );
    }
}
