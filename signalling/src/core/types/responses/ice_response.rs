use salvo::http::StatusCode;
use salvo::oapi::{self, EndpointOutRegister, ToSchema};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[salvo(schema(example = json!({"urls": ["turn:rtc.live.cloudflare.com:443?transport=tcp"], "username": "123123", "credential": "123123"})))]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[salvo(schema(example = json!(
    {
        "iceServers": [
          {
            "urls": [
              "stun:stun.cloudflare.com:3478",
              "stun:stun.cloudflare.com:53",
              "turn:turn.cloudflare.com:3478?transport=udp",
              "turn:turn.cloudflare.com:53?transport=udp",
              "turn:turn.cloudflare.com:3478?transport=tcp",
              "turn:turn.cloudflare.com:80?transport=tcp",
              "turns:turn.cloudflare.com:5349?transport=tcp",
              "turns:turn.cloudflare.com:443?transport=tcp"
            ],
            "username": "bc91b63e2b5d759f8eb9f3b58062439e0a0e15893d76317d833265ad08d6631099ce7c7087caabb31ad3e1c386424e3e",
            "credential": "ebd71f1d3edbc2b0edae3cd5a6d82284aeb5c3b8fdaa9b8e3bf9cec683e0d45fe9f5b44e5145db3300f06c250a15b4a0"
          }
        ]
      }
)))]
pub struct IceServersResponse {
    #[serde(rename = "iceServers")]
    pub ice_servers: Vec<IceServer>,
}

#[async_trait]
impl Writer for IceServersResponse {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.status_code(StatusCode::OK);
        res.render(Json(self));
    }
}

impl EndpointOutRegister for IceServersResponse {
    fn register(components: &mut oapi::Components, operation: &mut oapi::Operation) {
        operation.responses.insert(
            StatusCode::OK.as_str(),
            oapi::Response::new("OK").add_content(
                "application/json",
                IceServersResponse::to_schema(components),
            ),
        );
    }
}
