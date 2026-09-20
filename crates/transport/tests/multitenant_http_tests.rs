use axum::{
    Router,
    body::Body,
    extract::Form,
    extract::State,
    http::HeaderMap,
    http::{Request, StatusCode},
    response::Json,
    routing::{get, post},
};
use base64::Engine;
use oauth::{AuthorizeRequest, OAuthProvider, RegistrationRequest, TokenRequest};
use reqwest::Client;
use safety::CapabilityProfile;
use serde_json::json;
use sha2::Digest;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tenant::{TenantStore, UserId};
use tower::ServiceExt;
use transport::http::{HostedAuth, HttpConfig};
use transport::http_app::{router, router_with_app};
use url::Url;

async fn fake_token(
    Form(form): Form<HashMap<String, String>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if form.get("code").map(String::as_str) != Some("fixture-code") {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_grant"})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({"access_token":"fixture-github-token"})),
    )
}

async fn fake_user() -> Json<serde_json::Value> {
    Json(json!({"id":1001,"login":"fixture-user-a","name":"Fixture User A"}))
}

fn private_tempdir() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    directory
}

fn path_and_query(url: &Url) -> String {
    match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    }
}

fn pkce_request(client_id: &str, state: String) -> (AuthorizeRequest, String) {
    let verifier = "a-secret-verifier-that-is-long-enough-123456789".to_owned();
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()));
    (
        AuthorizeRequest {
            client_id: client_id.to_owned(),
            redirect_uri: "https://client.test/callback".into(),
            response_type: "code".into(),
            resource: "https://example.test/mcp".into(),
            scope: "mcp".into(),
            state,
            code_challenge: challenge,
            code_challenge_method: "S256".into(),
        },
        verifier,
    )
}

#[tokio::test]
async fn github_callback_resumes_original_redirect_and_pkce_transaction() {
    let github = Router::new()
        .route("/token", post(fake_token))
        .route("/user", get(fake_user));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let github_addr = listener.local_addr().unwrap();
    let github_task = tokio::spawn(async move {
        axum::serve(listener, github).await.unwrap();
    });

    let directory = private_tempdir();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let tenants = Arc::new(
        TenantStore::open(
            &directory.path().join("tenant.sqlite"),
            "fixture-encryption-key-material-that-is-long-enough",
        )
        .unwrap(),
    );
    let github_provider = Arc::new(identity::GitHubIdentityProvider::with_endpoints(
        "fixture-client-id".into(),
        secrecy::SecretString::from("fixture-client-secret"),
        Url::parse("https://example.test/auth/github/callback").unwrap(),
        Client::new(),
        Url::parse(&format!("http://{github_addr}/token")).unwrap(),
        Url::parse(&format!("http://{github_addr}/user")).unwrap(),
    ));
    let oauth = Arc::new(OAuthProvider::new(
        "https://example.test".into(),
        "/mcp".into(),
    ));
    let client = oauth
        .register(RegistrationRequest {
            redirect_uris: vec!["https://client.test/callback".into()],
            client_name: Some("fixture-client".into()),
        })
        .unwrap();
    let mut config = HttpConfig::for_tests();
    config.oauth = oauth.clone();
    config.hosted_auth = Some(Arc::new(HostedAuth {
        tenant: tenants.clone(),
        github: github_provider,
    }));
    let app = router(config);

    let state_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/state")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"client_id":client.client_id,"redirect_uri":"https://client.test/callback"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(state_response.status(), StatusCode::OK);
    let state_body = axum::body::to_bytes(state_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mcp_state = serde_json::from_slice::<serde_json::Value>(&state_body).unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned();
    let (request, verifier) = pkce_request(&client.client_id, mcp_state.clone());
    let mut authorize_url = Url::parse("https://example.test/oauth/authorize").unwrap();
    authorize_url
        .query_pairs_mut()
        .append_pair("client_id", &request.client_id)
        .append_pair("redirect_uri", &request.redirect_uri)
        .append_pair("response_type", &request.response_type)
        .append_pair("resource", &request.resource)
        .append_pair("scope", &request.scope)
        .append_pair("state", &request.state)
        .append_pair("code_challenge", &request.code_challenge)
        .append_pair("code_challenge_method", &request.code_challenge_method);
    let authorize_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(path_and_query(&authorize_url))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorize_response.status(), StatusCode::SEE_OTHER);
    let start_location = Url::parse(
        authorize_response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(start_location.path(), "/auth/github/start");
    assert_eq!(start_location.query_pairs().next().unwrap().0, "state");

    let start_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(path_and_query(&start_location))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_response.status(), StatusCode::SEE_OTHER);
    let cookie = start_response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert!(cookie.starts_with("mcp_github_state="));
    let github_location = Url::parse(
        start_response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(github_location.host_str(), Some("github.com"));
    let github_state = github_location
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .to_string();

    let callback_uri = format!(
        "/auth/github/callback?code=fixture-code&state={}",
        url::form_urlencoded::byte_serialize(github_state.as_bytes()).collect::<String>()
    );
    let callback_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(callback_uri.clone())
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(callback_response.status(), StatusCode::SEE_OTHER);
    let redirect = Url::parse(
        callback_response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        redirect.origin().ascii_serialization(),
        "https://client.test"
    );
    assert_eq!(redirect.path(), "/callback");
    assert_eq!(
        redirect
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1,
        mcp_state
    );
    let authorization_code = redirect
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .to_string();

    let user = tenants.get_user_by_github_id("1001").unwrap().unwrap();
    let token = oauth
        .exchange_code(TokenRequest {
            grant_type: "authorization_code".into(),
            code: authorization_code,
            redirect_uri: Some(request.redirect_uri),
            client_id: request.client_id,
            client_secret: Some(client.client_secret),
            code_verifier: Some(verifier),
            refresh_token: None,
            resource: Some(request.resource),
        })
        .unwrap();
    assert_eq!(
        oauth
            .verify_bearer_user(&token.access_token, "https://example.test/mcp")
            .unwrap(),
        user.id
    );

    let replay = app
        .oneshot(
            Request::builder()
                .uri(callback_uri)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
    github_task.abort();
}

#[derive(Clone, Default)]
struct CoolifyFixture {
    authorization: Arc<Mutex<Vec<String>>>,
}

async fn fixture_applications(
    State(fixture): State<CoolifyFixture>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("missing")
        .to_owned();
    fixture
        .authorization
        .lock()
        .unwrap()
        .push(authorization.clone());
    let suffix = authorization
        .strip_prefix("Bearer ")
        .unwrap_or("unknown")
        .replace('-', "_");
    Json(json!([{"uuid":"fixture-app","name":suffix}]))
}

struct TenantDispatchApp;
impl mcp_tools::McpApplication for TenantDispatchApp {
    fn tools(&self) -> Vec<mcp_tools::ToolSpec> {
        mcp_tools::registered_tools(CapabilityProfile::ReadOnly, None)
    }

    fn tools_for_user(&self, context: mcp_tools::TenantRequestContext) -> Vec<mcp_tools::ToolSpec> {
        mcp_tools::registered_tools(context.profile, None)
    }

    fn call<'a>(
        &'a self,
        _: &'a str,
        _: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = mcp_tools::ToolResult> + Send + 'a>>
    {
        Box::pin(async {
            mcp_tools::ToolResult {
                text: "{\"error\":\"global fallback\"}".into(),
                is_error: true,
            }
        })
    }

    fn call_for_user<'a>(
        &'a self,
        context: mcp_tools::TenantToolContext,
        name: &'a str,
        args: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = mcp_tools::ToolResult> + Send + 'a>>
    {
        Box::pin(async move { mcp_tools::call_tool_for_tenant(context, name, args).await })
    }
}

async fn start_fixture() -> (CoolifyFixture, String, tokio::task::JoinHandle<()>) {
    let fixture = CoolifyFixture::default();
    let app = Router::new()
        .route("/api/v1/applications", get(fixture_applications))
        .with_state(fixture.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (fixture, format!("http://{address}/"), task)
}

fn issue_user_token(
    oauth: &OAuthProvider,
    client: &oauth::RegistrationResponse,
    user_id: UserId,
    state_suffix: &str,
) -> String {
    let state = oauth
        .create_state(&client.client_id, "https://client.test/callback")
        .unwrap();
    let _ = state_suffix;
    let (request, verifier) = pkce_request(&client.client_id, state);
    let authorization = oauth.authorize_for_user(request, user_id).unwrap();
    oauth
        .exchange_code(TokenRequest {
            grant_type: "authorization_code".into(),
            code: authorization.code,
            redirect_uri: Some("https://client.test/callback".into()),
            client_id: client.client_id.clone(),
            client_secret: Some(client.client_secret.clone()),
            code_verifier: Some(verifier),
            refresh_token: None,
            resource: Some("https://example.test/mcp".into()),
        })
        .unwrap()
        .access_token
}

async fn list_tools(app: &Router, bearer: &str) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("authorization", format!("Bearer {bearer}"))
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .body(Body::from(
                    json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn call_mcp(
    app: &Router,
    bearer: &str,
    name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("authorization", format!("Bearer {bearer}"))
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc":"2.0",
                        "id":1,
                        "method":"tools/call",
                        "params":{"name":name,"arguments":arguments}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn user_a_calls_fixture_a_and_user_b_calls_fixture_b() {
    let (fixture, base_url, fixture_task) = start_fixture().await;
    let directory = private_tempdir();
    let tenants = Arc::new(
        TenantStore::open(
            &directory.path().join("tenant.sqlite"),
            "fixture-encryption-key-material-that-is-long-enough",
        )
        .unwrap(),
    );
    let user_a = tenants.upsert_user("2001", "fixture-a").unwrap();
    let user_b = tenants.upsert_user("2002", "fixture-b").unwrap();
    tenants
        .save_connection(
            user_a.id,
            &Url::parse(&base_url).unwrap(),
            &secrecy::SecretString::from("token-a"),
            CapabilityProfile::ReadOnly,
        )
        .unwrap();
    tenants
        .save_connection(
            user_b.id,
            &Url::parse(&base_url).unwrap(),
            &secrecy::SecretString::from("token-b"),
            CapabilityProfile::Operations,
        )
        .unwrap();
    let oauth = Arc::new(OAuthProvider::new(
        "https://example.test".into(),
        "/mcp".into(),
    ));
    let client = oauth
        .register(RegistrationRequest {
            redirect_uris: vec!["https://client.test/callback".into()],
            client_name: Some("fixture-client".into()),
        })
        .unwrap();
    let token_a = issue_user_token(&oauth, &client, user_a.id, "a");
    let token_b = issue_user_token(&oauth, &client, user_b.id, "b");
    let mut config = HttpConfig::for_tests();
    config.oauth = oauth;
    config.hosted_auth = Some(Arc::new(HostedAuth {
        tenant: tenants,
        github: Arc::new(identity::GitHubIdentityProvider::new(
            "fixture-client".into(),
            secrecy::SecretString::from("fixture-client-secret"),
            Url::parse("https://example.test/auth/github/callback").unwrap(),
            Client::new(),
        )),
    }));
    let app = router_with_app(config, TenantDispatchApp);

    let tools_a = list_tools(&app, &token_a).await;
    let tools_b = list_tools(&app, &token_b).await;
    let names_a: Vec<&str> = tools_a["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let names_b: Vec<&str> = tools_b["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(!names_a.contains(&"application"));
    assert!(names_b.contains(&"application"));

    let body_a = call_mcp(&app, &token_a, "list_applications", json!({})).await;
    let body_b = call_mcp(&app, &token_b, "list_applications", json!({})).await;
    let text_a = body_a["result"]["content"][0]["text"].as_str().unwrap();
    let text_b = body_b["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text_a.contains("token_a"));
    assert!(text_b.contains("token_b"));
    assert_ne!(text_a, text_b);
    assert_eq!(fixture.authorization.lock().unwrap().len(), 2);
    assert!(!text_a.contains("token_b"));
    assert!(!text_b.contains("token_a"));
    fixture_task.abort();
}

#[tokio::test]
async fn missing_or_undecryptable_connection_fails_without_global_fallback() {
    let (fixture, base_url, fixture_task) = start_fixture().await;
    let directory = private_tempdir();
    let database = directory.path().join("tenant.sqlite");
    let tenants = Arc::new(
        TenantStore::open(
            &database,
            "fixture-encryption-key-material-that-is-long-enough",
        )
        .unwrap(),
    );
    let missing_user = tenants.upsert_user("3001", "fixture-missing").unwrap();
    let encrypted_user = tenants.upsert_user("3002", "fixture-encrypted").unwrap();
    tenants
        .save_connection(
            encrypted_user.id,
            &Url::parse(&base_url).unwrap(),
            &secrecy::SecretString::from("token-encrypted"),
            CapabilityProfile::ReadOnly,
        )
        .unwrap();
    let oauth = Arc::new(OAuthProvider::new(
        "https://example.test".into(),
        "/mcp".into(),
    ));
    let client = oauth
        .register(RegistrationRequest {
            redirect_uris: vec!["https://client.test/callback".into()],
            client_name: Some("fixture-client".into()),
        })
        .unwrap();
    let missing_token = issue_user_token(&oauth, &client, missing_user.id, "missing");
    let encrypted_token = issue_user_token(&oauth, &client, encrypted_user.id, "encrypted");
    let wrong_key = Arc::new(
        TenantStore::open(&database, "a-different-fixture-encryption-key-material").unwrap(),
    );
    let mut config = HttpConfig::for_tests();
    config.oauth = oauth;
    config.hosted_auth = Some(Arc::new(HostedAuth {
        tenant: wrong_key,
        github: Arc::new(identity::GitHubIdentityProvider::new(
            "fixture-client".into(),
            secrecy::SecretString::from("fixture-client-secret"),
            Url::parse("https://example.test/auth/github/callback").unwrap(),
            Client::new(),
        )),
    }));
    let app = router_with_app(config, TenantDispatchApp);
    let missing_body = call_mcp(&app, &missing_token, "list_applications", json!({})).await;
    let missing_text = missing_body["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(missing_text.contains("tenant connection unavailable"));
    let encrypted_body = call_mcp(&app, &encrypted_token, "list_applications", json!({})).await;
    let encrypted_text = encrypted_body["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(encrypted_text.contains("tenant connection unavailable"));
    assert!(!missing_text.contains("token-a"));
    assert!(!encrypted_text.contains("token-encrypted"));
    assert!(fixture.authorization.lock().unwrap().is_empty());
    fixture_task.abort();
}

#[tokio::test]
async fn user_profile_cannot_be_elevated_by_tool_arguments() {
    let (fixture, base_url, fixture_task) = start_fixture().await;
    let directory = private_tempdir();
    let tenants = Arc::new(
        TenantStore::open(
            &directory.path().join("tenant.sqlite"),
            "fixture-encryption-key-material-that-is-long-enough",
        )
        .unwrap(),
    );
    let user = tenants.upsert_user("4001", "fixture-read-only").unwrap();
    tenants
        .save_connection(
            user.id,
            &Url::parse(&base_url).unwrap(),
            &secrecy::SecretString::from("token-read-only"),
            CapabilityProfile::ReadOnly,
        )
        .unwrap();
    let oauth = Arc::new(OAuthProvider::new(
        "https://example.test".into(),
        "/mcp".into(),
    ));
    let client = oauth
        .register(RegistrationRequest {
            redirect_uris: vec!["https://client.test/callback".into()],
            client_name: Some("fixture-client".into()),
        })
        .unwrap();
    let token = issue_user_token(&oauth, &client, user.id, "profile");
    let mut config = HttpConfig::for_tests();
    config.oauth = oauth;
    config.hosted_auth = Some(Arc::new(HostedAuth {
        tenant: tenants,
        github: Arc::new(identity::GitHubIdentityProvider::new(
            "fixture-client".into(),
            secrecy::SecretString::from("fixture-client-secret"),
            Url::parse("https://example.test/auth/github/callback").unwrap(),
            Client::new(),
        )),
    }));
    let app = router_with_app(config, TenantDispatchApp);
    let body = call_mcp(
        &app,
        &token,
        "application",
        json!({"uuid":"app-1","action":"delete","profile":"admin"}),
    )
    .await;
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("is_error"));
    assert!(text.contains("invalid typed arguments") || text.contains("not permitted"));
    assert!(fixture.authorization.lock().unwrap().is_empty());
    fixture_task.abort();
}
