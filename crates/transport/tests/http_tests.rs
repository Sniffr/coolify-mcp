use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::net::SocketAddr;
use tower::{Service, ServiceExt};
use transport::http::{DRAIN_TIMEOUT, HTTP2_SUPPORTED, HttpConfig, normalize_public_url};
use transport::http_app::{router, router_with_app};
const _: () = assert!(!HTTP2_SUPPORTED);

struct TestApp;
impl mcp_tools::McpApplication for TestApp {
    fn tools(&self) -> Vec<mcp_tools::ToolSpec> {
        Vec::new()
    }
    fn call<'a>(
        &'a self,
        _: &'a str,
        _: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = mcp_tools::ToolResult> + Send + 'a>>
    {
        Box::pin(async {
            mcp_tools::ToolResult {
                text: String::new(),
                is_error: false,
            }
        })
    }
}

#[test]
fn timeout_configuration_keeps_header_and_request_deadlines_distinct() {
    assert_eq!(DRAIN_TIMEOUT, std::time::Duration::from_secs(30));
    let config = HttpConfig::for_tests();
    assert_eq!(config.header_timeout, std::time::Duration::from_secs(15));
    assert_eq!(config.request_timeout, std::time::Duration::from_secs(30));
    assert!(config.header_timeout < config.request_timeout);
}

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
async fn hosted_tool_call_audits_rejection_without_args_or_secrets() {
    let path = std::env::temp_dir().join(format!("audit-{}.jsonl", uuid::Uuid::new_v4()));
    let mut config = HttpConfig::for_tests();
    config.audit_path = path.clone();
    let app = router_with_app(config, TestApp);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/mcp")
                .method("POST")
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"cloud_tokens","arguments":{"uuid":"resource-123","token":"super-secret"}}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let audit = std::fs::read_to_string(&path).unwrap();
    assert!(audit.contains("cloud_tokens"));
    assert!(audit.contains("resource-123"));
    assert!(audit.contains("rejected"));
    assert!(!audit.contains("super-secret"));
    assert!(!audit.contains("arguments"));
    assert!(!audit.contains("uuid"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn oauth_mutation_endpoints_rate_limit_by_client_ip() {
    let app = router(HttpConfig::for_tests());
    let mut limited = false;
    for index in 0..31 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/oauth/register")
                    .method("POST")
                    .header("content-type", "application/json")
                    .header("x-forwarded-for", format!("198.51.100.{}", index + 10))
                    .body(Body::from(
                        r#"{"redirect_uris":["https://client.test/callback"]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            limited = true;
            break;
        }
    }
    assert!(limited);
}

#[tokio::test]
async fn peer_ip_rate_buckets_ignore_ports_and_forwarded_ip_spoofing() {
    let app = router_with_app(HttpConfig::for_tests(), TestApp);
    let mut make = app.into_make_service_with_connect_info::<SocketAddr>();
    let mut peer_one = make
        .call("127.0.0.1:10001".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    let mut same_ip_different_port = make
        .call("127.0.0.1:10002".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    let mut distinct_ip = make
        .call("192.0.2.10:10001".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    for index in 0..30 {
        let request = Request::builder()
            .uri("/oauth/register")
            .method("POST")
            .header("content-type", "application/json")
            .header("x-forwarded-for", format!("203.0.113.{index}"))
            .body(Body::from(
                r#"{"redirect_uris":["https://client.test/callback"]}"#,
            ))
            .unwrap();
        assert_ne!(
            peer_one.call(request).await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
    let request = Request::builder()
        .uri("/oauth/register")
        .method("POST")
        .header("content-type", "application/json")
        .header("x-forwarded-for", "198.51.100.77")
        .body(Body::from(
            r#"{"redirect_uris":["https://client.test/callback"]}"#,
        ))
        .unwrap();
    assert_eq!(
        same_ip_different_port.call(request).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let request = Request::builder()
        .uri("/oauth/register")
        .method("POST")
        .header("content-type", "application/json")
        .header("x-forwarded-for", "198.51.100.78")
        .body(Body::from(
            r#"{"redirect_uris":["https://client.test/callback"]}"#,
        ))
        .unwrap();
    assert_ne!(
        distinct_ip.call(request).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn health_reports_degraded_persistence_without_hiding_discovery() {
    let config = HttpConfig::for_tests();
    config
        .persistence_health
        .store(false, std::sync::atomic::Ordering::Release);
    let app = router(config);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
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
