use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;

use super::room_response::RoomResponse;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListRoomResponse {
    pub rooms: Vec<RoomResponse>,
}

#[async_trait]
impl Writer for ListRoomResponse {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.status_code(StatusCode::OK);
        res.render(Json(self));
    }
}

impl EndpointOutRegister for ListRoomResponse {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::OK.as_str(),
            oapi::Response::new("OK")
                .add_content("application/json", ListRoomResponse::to_schema(components)),
        );
    }
}
