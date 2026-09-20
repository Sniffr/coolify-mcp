use identity::{GitHubIdentityProvider, GitHubUser};
use secrecy::SecretString;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};
use url::Url;

const CLIENT_SECRET: &str = "fixture-client-secret";

#[derive(Clone, Copy)]
enum FixtureMode {
    Happy,
    InvalidCode,
    MalformedIdentity,
}

struct Fixture {
    base_url: Url,
    task: JoinHandle<()>,
}

impl Fixture {
    async fn start(mode: FixtureMode) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let expected_requests = match mode {
            FixtureMode::InvalidCode => 1,
            FixtureMode::Happy | FixtureMode::MalformedIdentity => 2,
        };
        let task = tokio::spawn(async move {
            for request_number in 0..expected_requests {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream).await;
                let is_token_exchange = request.starts_with("POST /login/oauth/access_token ");
                let is_user_fetch = request.starts_with("GET /user ");
                let response = match (mode, request_number, is_token_exchange, is_user_fetch) {
                    (FixtureMode::Happy, 0, true, false) => json_response(
                        "200 OK",
                        r#"{"access_token":"github-fixture-token","token_type":"bearer","scope":"read:user user:email"}"#,
                    ),
                    (FixtureMode::Happy, 1, false, true) => json_response(
                        "200 OK",
                        r#"{"id":12345,"login":"fixture-user","name":"Fixture User","email":"fixture@example.invalid"}"#,
                    ),
                    (FixtureMode::InvalidCode, 0, true, false) => json_response(
                        "400 Bad Request",
                        r#"{"error":"invalid_grant","error_description":"fixture-secret-body"}"#,
                    ),
                    (FixtureMode::MalformedIdentity, 0, true, false) => json_response(
                        "200 OK",
                        r#"{"access_token":"github-fixture-token","token_type":"bearer"}"#,
                    ),
                    (FixtureMode::MalformedIdentity, 1, false, true) => json_response(
                        "200 OK",
                        r#"{"id":"not-a-github-id","login":null,"name":"fixture-secret-body"}"#,
                    ),
                    _ => json_response(
                        "500 Internal Server Error",
                        r#"{"error":"unexpected request"}"#,
                    ),
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        Self {
            base_url: Url::parse(&format!("http://{address}/")).unwrap(),
            task,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await.unwrap();
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if bytes.len() > 16 * 1024 {
            break;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn json_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn test_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

fn provider(fixture: &Fixture) -> GitHubIdentityProvider {
    let token_url = fixture.base_url.join("login/oauth/access_token").unwrap();
    let user_url = fixture.base_url.join("user").unwrap();
    GitHubIdentityProvider::with_endpoints(
        "fixture-client-id".to_owned(),
        SecretString::from(CLIENT_SECRET.to_owned()),
        Url::parse("https://service.example/auth/github/callback").unwrap(),
        test_client(),
        token_url,
        user_url,
    )
}

#[tokio::test]
async fn callback_exchanges_code_and_fetches_minimal_identity() {
    let fixture = Fixture::start(FixtureMode::Happy).await;
    let result = provider(&fixture)
        .exchange_callback("fixture-code")
        .await
        .unwrap();

    assert_eq!(
        result,
        GitHubUser {
            github_id: "12345".into(),
            login: "fixture-user".into(),
            display_name: Some("Fixture User".into()),
        }
    );
}

#[tokio::test]
async fn github_error_and_malformed_identity_fail_without_body_leakage() {
    let fixture = Fixture::start(FixtureMode::InvalidCode).await;
    let error = provider(&fixture)
        .exchange_callback("invalid-fixture-code")
        .await
        .unwrap_err();
    let message = error.to_string();
    assert!(!message.contains("fixture-secret-body"));
    assert!(!message.contains("github-fixture-token"));

    let fixture = Fixture::start(FixtureMode::MalformedIdentity).await;
    let error = provider(&fixture)
        .exchange_callback("fixture-code")
        .await
        .unwrap_err();
    let message = error.to_string();
    assert!(!message.contains("fixture-secret-body"));
    assert!(!message.contains("github-fixture-token"));
}

#[test]
fn authorization_url_requests_only_identity_scopes() {
    let provider = GitHubIdentityProvider::new(
        "fixture-client-id".to_owned(),
        SecretString::from(CLIENT_SECRET.to_owned()),
        Url::parse("https://service.example/auth/github/callback").unwrap(),
        test_client(),
    );
    let url = provider.authorization_url("state-fixture");
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();

    assert_eq!(
        query.get("client_id"),
        Some(&"fixture-client-id".to_owned())
    );
    assert_eq!(query.get("state"), Some(&"state-fixture".to_owned()));
    assert_eq!(
        query.get("redirect_uri"),
        Some(&"https://service.example/auth/github/callback".to_owned())
    );
    assert_eq!(query.get("scope"), Some(&"read:user".to_owned()));
    assert!(!url.as_str().contains("user:email"));
    assert!(!url.as_str().contains("repo"));
    assert!(!url.as_str().contains("admin"));
}

#[tokio::test]
async fn oversized_github_error_body_is_bounded_and_redacted() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = read_request(&mut stream).await;
        let body = format!(r#"{{"error":"{}"}}"#, "fixture-secret-body".repeat(10_000));
        stream
            .write_all(json_response("400 Bad Request", &body).as_bytes())
            .await
            .unwrap();
    });
    let fixture = Fixture {
        base_url: Url::parse(&format!("http://{address}/")).unwrap(),
        task,
    };
    let error = provider(&fixture)
        .exchange_callback("fixture-code")
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("fixture-secret-body"));
}
