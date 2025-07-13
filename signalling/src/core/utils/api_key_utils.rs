use salvo::Handler;
use salvo::prelude::*;

use crate::core::env::app_env::AppEnv;
use crate::core::types::errors::auth_error::AuthError;

pub fn api_key_middleware() -> impl Handler {
    #[handler]
    async fn middleware(req: &mut Request, depot: &mut Depot, res: &mut Response) {
        let api_key_header = req.headers().get("X-API-Key").and_then(|h| h.to_str().ok());

        if let Some(key) = api_key_header {
            let app_env = depot.obtain::<AppEnv>().unwrap();

            if key != app_env.client_api_key {
                res.status_code(StatusCode::UNAUTHORIZED);
                return res.render(Json(AuthError::InvalidAPIKey));
            }
        } else {
            res.status_code(StatusCode::UNAUTHORIZED);
            return res.render(Json(AuthError::InvalidAPIKey));
        }
    }
    middleware
}
