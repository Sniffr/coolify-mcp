use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

use coolify_api::{CoolifyClient, CoolifyConfig, TokenSource};
use url::Url;

fn client_for(url: &str, token: &str) -> CoolifyClient {
    CoolifyClient::new(CoolifyConfig {
        base_url: Url::parse(url).unwrap(),
        token_source: TokenSource::from_env(&HashMap::from([(
            "COOLIFY_ACCESS_TOKEN".into(),
            token.into(),
        )]))
        .unwrap(),
        custom_headers: Default::default(),
        timeout: Duration::from_secs(45),
    })
    .unwrap()
}

fn serve_once(status: &str, extra_headers: &str, body: &str) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
        body.len()
    );
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 16_384];
        let _ = stream.read(&mut request);
        let _ = stream.write_all(response.as_bytes());
    });
    url
}

#[tokio::test]
async fn probe_preserves_redirect_metadata_without_following() {
    let url = serve_once(
        "302 Found",
        "Content-Type: text/html\r\nLocation: /login\r\n",
        "Just a moment, verifying your browser",
    );
    let outcome = client_for(&url, "probe-token")
        .probe_get("/__coolify_mcp_doctor_invalid__")
        .await
        .unwrap();
    assert_eq!(outcome.status, 302);
    assert!(outcome.redirected);
    assert_eq!(outcome.location.as_deref(), Some("/login"));
    assert!(
        outcome
            .content_type
            .as_deref()
            .is_some_and(|v| v.contains("text/html"))
    );
    assert!(outcome.body.contains("Just a moment"));
}

#[tokio::test]
async fn probe_preserves_html_body_without_html_marker() {
    let body = "<head><title>Sign in</title></head>";
    let url = serve_once("200 OK", "Content-Type: text/html\r\n", body);
    let outcome = client_for(&url, "probe-token")
        .probe_get("/__coolify_mcp_doctor_invalid__")
        .await
        .unwrap();
    assert_eq!(outcome.status, 200);
    assert!(!outcome.redirected);
    assert!(outcome.location.is_none());
    assert_eq!(outcome.body, body);
}

#[tokio::test]
async fn probe_returns_error_statuses_with_real_content_type() {
    let url = serve_once(
        "404 Not Found",
        "Content-Type: application/json\r\n",
        "{\"message\":\"not found\"}",
    );
    let outcome = client_for(&url, "probe-token")
        .probe_get("/__coolify_mcp_doctor_invalid__")
        .await
        .unwrap();
    assert_eq!(outcome.status, 404);
    assert!(
        outcome
            .content_type
            .as_deref()
            .is_some_and(|v| v.contains("application/json"))
    );
}

#[tokio::test]
async fn probe_redacts_token_and_reports_transport_failures_without_secrets() {
    let outcome = client_for("http://127.0.0.1:1", "super-secret-token")
        .probe_get("/version")
        .await;
    let error = outcome.unwrap_err();
    assert!(!error.contains("super-secret-token"));
}
