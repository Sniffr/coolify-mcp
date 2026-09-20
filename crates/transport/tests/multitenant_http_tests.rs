use axum::{
    Router,
    body::Body,
    extract::Form,
    http::{Request, StatusCode},
    response::Json,
    routing::{get, post},
};
use base64::Engine;
use oauth::{AuthorizeRequest, OAuthProvider, RegistrationRequest, TokenRequest};
use reqwest::Client;
use serde_json::json;
use sha2::Digest;
use std::{collections::HashMap, sync::Arc};
use tenant::TenantStore;
use tower::ServiceExt;
use transport::http::{HostedAuth, HttpConfig};
use transport::http_app::router;
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

    let directory = tempfile::tempdir().unwrap();
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
