use salvo::prelude::*;

#[handler]
async fn health_check(res: &mut Response) {
    res.render("[v3] Waterbus Service written in Rust");
}

pub async fn get_api_router() -> Router {
    let router = Router::with_path("/health_check").goal(health_check);

    router
}
