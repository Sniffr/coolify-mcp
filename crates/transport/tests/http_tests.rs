use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::{collections::HashMap, net::SocketAddr};
use tower::{Service, ServiceExt};
use transport::http::{
    DRAIN_TIMEOUT, HTTP2_SUPPORTED, HttpConfig, normalize_public_url, validate_hosted_environment,
};
use transport::http_app::{router, router_with_app};
const _: () = assert!(!HTTP2_SUPPORTED);

fn hosted_env() -> HashMap<String, String> {
    HashMap::from([
        ("MCP_PUBLIC_URL".into(), "https://mcp.example.test".into()),
        ("MCP_DATABASE_PATH".into(), "/data/tenant.sqlite".into()),
        (
            "MCP_CONNECTION_ENCRYPTION_KEY".into(),
            "fixture-encryption-key".into(),
        ),
        ("GITHUB_CLIENT_ID".into(), "fixture-client-id".into()),
        (
            "GITHUB_CLIENT_SECRET".into(),
            "fixture-client-secret".into(),
        ),
        (
            "GITHUB_CALLBACK_URL".into(),
            "https://mcp.example.test/auth/github/callback".into(),
        ),
    ])
}

#[test]
fn hosted_configuration_requires_every_persistence_and_identity_variable() {
    let required = [
        "MCP_PUBLIC_URL",
        "MCP_DATABASE_PATH",
        "MCP_CONNECTION_ENCRYPTION_KEY",
        "GITHUB_CLIENT_ID",
        "GITHUB_CLIENT_SECRET",
        "GITHUB_CALLBACK_URL",
    ];
    for variable in required {
        let mut env = hosted_env();
        env.remove(variable);
        assert!(
            validate_hosted_environment(&env).is_err(),
            "{variable} must be required"
        );
    }
}

#[test]
fn hosted_configuration_does_not_require_global_coolify_credentials() {
    let env = hosted_env();
    assert!(validate_hosted_environment(&env).is_ok());
}

#[test]
fn hosted_callback_must_match_public_url_without_leaking_values() {
    let mut env = hosted_env();
    env.insert(
        "GITHUB_CALLBACK_URL".into(),
        "https://other.example/callback".into(),
    );
    let error = validate_hosted_environment(&env).unwrap_err().to_string();
    assert!(error.contains("GITHUB_CALLBACK_URL"));
    assert!(!error.contains("other.example"));
}

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

fn token_pkce() -> (String, String) {
    use base64::Engine;
    use sha2::Digest;
    let verifier = "test-verifier-that-is-long-enough-0123456789".to_owned();
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

async fn token_test_app() -> (
    tower::util::BoxCloneService<Request<Body>, axum::response::Response, std::convert::Infallible>,
    std::sync::Arc<oauth::OAuthProvider>,
    oauth::RegistrationResponse,
) {
    use std::net::SocketAddr;
    use tower::Service;
    let config = HttpConfig::for_tests();
    let oauth = config.oauth.clone();
    let client = oauth
        .register(oauth::RegistrationRequest {
            redirect_uris: vec!["https://client.test/callback".into()],
            client_name: Some("token-compat-client".into()),
        })
        .unwrap();
    let app = router_with_app(config, TestApp);
    let mut make = app.into_make_service_with_connect_info::<SocketAddr>();
    let peer: SocketAddr = "127.0.0.1:40001".parse().unwrap();
    let svc = make.call(peer).await.unwrap();
    (tower::util::BoxCloneService::new(svc), oauth, client)
}

fn mint_code(
    oauth: &oauth::OAuthProvider,
    client: &oauth::RegistrationResponse,
) -> (String, String) {
    let state = oauth
        .create_state(&client.client_id, "https://client.test/callback")
        .unwrap();
    let (verifier, challenge) = token_pkce();
    let authorization = oauth
        .authorize(oauth::AuthorizeRequest {
            client_id: client.client_id.clone(),
            redirect_uri: "https://client.test/callback".into(),
            response_type: "code".into(),
            resource: "https://example.test/mcp".into(),
            scope: "mcp".into(),
            state,
            code_challenge: challenge,
            code_challenge_method: "S256".into(),
        })
        .unwrap();
    (authorization.code, verifier)
}

async fn post_token(
    svc: &mut tower::util::BoxCloneService<
        Request<Body>,
        axum::response::Response,
        std::convert::Infallible,
    >,
    content_type: &str,
    extra_headers: &[(&str, String)],
    body: String,
) -> (StatusCode, serde_json::Value, String) {
    use tower::Service;
    let mut builder = Request::builder()
        .uri("/oauth/token")
        .method("POST")
        .header("content-type", content_type);
    for (name, value) in extra_headers {
        builder = builder.header(*name, value.as_str());
    }
    let response = svc
        .call(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let raw = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    (status, json, text)
}

#[tokio::test]
async fn token_endpoint_accepts_json_body() {
    let (mut svc, oauth, client) = token_test_app().await;
    let (code, verifier) = mint_code(&oauth, &client);
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": "https://client.test/callback",
        "client_id": client.client_id,
        "client_secret": client.client_secret,
        "code_verifier": verifier,
        "resource": "https://example.test/mcp",
    })
    .to_string();
    let (status, json, _) = post_token(&mut svc, "application/json", &[], body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json.get("access_token").is_some());
    assert!(json.get("refresh_token").is_some());
}

#[tokio::test]
async fn token_endpoint_accepts_basic_auth_client_credentials() {
    use base64::Engine;
    let (mut svc, oauth, client) = token_test_app().await;
    let (code, verifier) = mint_code(&oauth, &client);
    let credentials = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", client.client_id, client.client_secret));
    // No client_id/client_secret in the form body; they arrive via Basic auth.
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", &code)
        .append_pair("redirect_uri", "https://client.test/callback")
        .append_pair("code_verifier", &verifier)
        .append_pair("resource", "https://example.test/mcp")
        .finish();
    let (status, json, _) = post_token(
        &mut svc,
        "application/x-www-form-urlencoded",
        &[("authorization", format!("Basic {credentials}"))],
        body,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(json.get("access_token").is_some());
}

#[tokio::test]
async fn token_endpoint_returns_json_error_instead_of_422_text() {
    let (mut svc, _oauth, _client) = token_test_app().await;
    // Form body missing client_id entirely (the Claude Code failure mode).
    let body = "grant_type=authorization_code&code=whatever".to_owned();
    let (status, json, text) =
        post_token(&mut svc, "application/x-www-form-urlencoded", &[], body).await;
    assert_ne!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        json.get("error").is_some(),
        "expected JSON error, got: {text}"
    );
    assert!(!text.contains("Failed to deserialize"));

    // Malformed JSON body must also be a JSON OAuth error, not plain text.
    let (status, json, text) = post_token(
        &mut svc,
        "application/json",
        &[],
        "{not valid json".to_owned(),
    )
    .await;
    assert_ne!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        json.get("error").is_some(),
        "expected JSON error, got: {text}"
    );
}

#[tokio::test]
async fn refresh_token_grant_allows_missing_client_id() {
    let (mut svc, oauth, client) = token_test_app().await;
    let (code, verifier) = mint_code(&oauth, &client);
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", &code)
        .append_pair("redirect_uri", "https://client.test/callback")
        .append_pair("client_id", &client.client_id)
        .append_pair("client_secret", &client.client_secret)
        .append_pair("code_verifier", &verifier)
        .append_pair("resource", "https://example.test/mcp")
        .finish();
    let (status, json, _) =
        post_token(&mut svc, "application/x-www-form-urlencoded", &[], body).await;
    assert_eq!(status, StatusCode::OK);
    let refresh_token = json["refresh_token"].as_str().unwrap().to_owned();

    // Refresh with no client_id in body and no Basic header.
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "refresh_token")
        .append_pair("refresh_token", &refresh_token)
        .finish();
    let (status, json, text) =
        post_token(&mut svc, "application/x-www-form-urlencoded", &[], body).await;
    assert_ne!(status, StatusCode::UNPROCESSABLE_ENTITY, "{text}");
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(json.get("access_token").is_some());
}
