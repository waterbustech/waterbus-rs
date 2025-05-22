use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;

#[derive(Serialize, ToSchema)]
pub struct PresignedResponse {
    #[serde(rename = "presignedUrl")]
    pub presigned_url: String,
}

#[async_trait]
impl Writer for PresignedResponse {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.status_code(StatusCode::CREATED);
        res.render(Json(self));
    }
}

impl EndpointOutRegister for PresignedResponse {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::CREATED.as_str(),
            oapi::Response::new("Created")
                .add_content("application/json", PresignedResponse::to_schema(components)),
        );
    }
}
