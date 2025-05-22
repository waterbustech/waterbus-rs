use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;

use crate::core::entities::models::User;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub token: String,
    pub refresh_token: String,
    pub user: Option<User>,
}

#[async_trait]
impl Writer for AuthResponse {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        if self.user.is_some() {
            res.status_code(StatusCode::CREATED);
            res.render(Json(self));
        } else {
            res.status_code(StatusCode::OK);
            res.render(Json(self));
        }
    }
}

impl EndpointOutRegister for AuthResponse {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::OK.as_str(),
            oapi::Response::new("OK")
                .add_content("application/json", AuthResponse::to_schema(components)),
        );
        operation.responses.insert(
            StatusCode::CREATED.as_str(),
            oapi::Response::new("Created")
                .add_content("application/json", AuthResponse::to_schema(components)),
        );
    }
}
