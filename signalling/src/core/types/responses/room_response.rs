use salvo::http::{Method, StatusCode};
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::Serialize;

use crate::core::entities::models::{Member, Participant, Room, User};

use super::message_response::MessageResponse;

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RoomResponse {
    #[serde(flatten)]
    pub room: Room,
    pub members: Vec<MemberResponse>,
    pub participants: Vec<ParticipantResponse>,
    pub latest_message: Option<MessageResponse>,
    pub is_protected: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemberResponse {
    #[serde(flatten)]
    pub member: Member,
    pub user: Option<User>,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantResponse {
    #[serde(flatten)]
    pub participant: Participant,
    pub user: Option<User>,
}

#[async_trait]
impl Writer for RoomResponse {
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

impl EndpointOutRegister for RoomResponse {
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
