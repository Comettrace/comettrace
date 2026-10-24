use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;

pub fn health_router() -> Router {
    Router::new().route("/health", get(alive))
}

async fn alive() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::health_router;

    #[tokio::test]
    async fn being_asked_whether_we_are_alive_needs_no_credentials() {
        let request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .expect("a request");

        let response = health_router().oneshot(request).await.expect("a response");

        assert_eq!(response.status(), StatusCode::OK);
    }
}
