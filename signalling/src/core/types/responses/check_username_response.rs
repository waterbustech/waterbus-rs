use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckUsernameResponse {
    pub is_registered: bool,
}

#[async_trait]
impl Writer for CheckUsernameResponse {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.status_code(StatusCode::OK);
        res.render(Json(self));
    }
}

impl EndpointOutRegister for CheckUsernameResponse {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::OK.as_str(),
            oapi::Response::new("OK").add_content(
                "application/json",
                CheckUsernameResponse::to_schema(components),
            ),
        );
    }
}
