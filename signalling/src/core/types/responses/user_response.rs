use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister};
use salvo::prelude::*;

use crate::core::entities::models::User;

#[async_trait]
impl Writer for User {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.status_code(StatusCode::OK);
        res.render(Json(self));
    }
}

impl EndpointOutRegister for User {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::OK.as_str(),
            oapi::Response::new("OK").add_content("application/json", User::to_schema(components)),
        );
    }
}
