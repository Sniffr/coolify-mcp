use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use transport::http::{HttpConfig, normalize_public_url};
use transport::http_app::router;

#[test]
fn normalizes_public_url_and_rejects_insecure_urls() {
    assert_eq!(
        normalize_public_url("https://example.test/")
            .unwrap()
            .as_str(),
        "https://example.test/"
    );
    assert!(normalize_public_url("http://example.test").is_err());
}

#[tokio::test]
async fn health_and_discovery_are_public_but_mcp_requires_bearer() {
    let app = router(HttpConfig::for_tests());
    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    let discovery = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-authorization-server")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(discovery.status(), StatusCode::OK);
    let mcp = app
        .oneshot(
            Request::builder()
                .uri("/mcp")
                .method("POST")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mcp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn oversized_body_is_rejected() {
    let app = router(HttpConfig::for_tests());
    let req = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("authorization", "Bearer bad")
        .body(Body::from(vec![b'x'; 5 * 1024 * 1024 + 1]))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
