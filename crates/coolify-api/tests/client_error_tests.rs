use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use coolify_api::{CoolifyApiError, CoolifyClient, CoolifyConfig, HttpErrorDetails, TokenSource};
use reqwest::{
    Method,
    header::{CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde_json::json;
use url::Url;

fn client_for(url: &str, token: &str, custom_headers: HeaderMap) -> CoolifyClient {
    CoolifyClient::new(CoolifyConfig {
        base_url: Url::parse(url).unwrap(),
        token_source: TokenSource::from_env(&std::collections::HashMap::from([(
            "COOLIFY_ACCESS_TOKEN".into(),
            token.into(),
        )]))
        .unwrap(),
        custom_headers,
        timeout: std::time::Duration::from_secs(45),
    })
    .unwrap()
}

fn server(
    status: u16,
    content_type: &str,
    body: &str,
    extra: &str,
) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let body = body.to_owned();
    let content_type = content_type.to_owned();
    let extra = extra.to_owned();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 16_384];
        let bytes = stream.read(&mut request).unwrap();
        let response = format!(
            "HTTP/1.1 {} Test\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}\r\n{}",
            status,
            content_type,
            body.len(),
            extra,
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request[..bytes].to_vec()).unwrap()
    });
    (url, handle)
}

#[test]
fn http_errors_retain_status_retry_after_and_bounded_body() {
    let details = HttpErrorDetails::new(429, "x".repeat(20_000), Some("7".into()));
    let error = CoolifyApiError::http(details);
    let text = error.to_string();
    assert!(text.contains("429"));
    assert!(text.contains("Retry-After"));
    assert!(text.len() < 12_000);
    assert_eq!(error.retry_after(), Some("7"));
}

#[test]
fn errors_do_not_debug_or_display_secrets() {
    let token = "bearer-secret";
    let error = CoolifyApiError::http(HttpErrorDetails::new(
        401,
        format!("upstream echoed Authorization: Bearer {token}"),
        None,
    ));
    assert!(!format!("{error:?}").contains(token));
    assert!(!format!("{error}").contains(token));
    for error in [
        CoolifyApiError::Config(format!("token={token}")),
        CoolifyApiError::Transport(format!("token={token}")),
        CoolifyApiError::Decode(format!("token={token}")),
    ] {
        assert!(!format!("{error:?}").contains(token));
        assert!(!format!("{error}").contains(token));
    }
}

#[tokio::test]
async fn client_decodes_json_text_empty_and_gates_json_by_content_type() {
    let (url, handle) = server(200, "application/json", r#"{"ok":true}"#, "");
    let client = client_for(&url, "token", HeaderMap::new());
    let value: serde_json::Value = client
        .request_json(Method::GET, "/test", None)
        .await
        .unwrap();
    assert_eq!(value, json!({"ok": true}));
    handle.join().unwrap();

    let (url, handle) = server(200, "text/plain", "hello", "");
    let text = client_for(&url, "token", HeaderMap::new())
        .request_text(Method::GET, "/test")
        .await
        .unwrap();
    assert_eq!(text, "hello");
    handle.join().unwrap();

    let (url, handle) = server(200, "text/plain", "", "");
    let text = client_for(&url, "token", HeaderMap::new())
        .request_text(Method::GET, "/test")
        .await
        .unwrap();
    assert!(text.is_empty());
    handle.join().unwrap();

    let (url, handle) = server(200, "text/plain", "not json", "");
    let error = client_for(&url, "token", HeaderMap::new())
        .request_json::<serde_json::Value>(Method::GET, "/test", None)
        .await
        .unwrap_err();
    assert!(matches!(error, CoolifyApiError::Decode(_)));
    handle.join().unwrap();
}

#[tokio::test]
async fn client_protects_auth_and_content_type_headers() {
    let mut custom = HeaderMap::new();
    custom.insert("x-custom", HeaderValue::from_static("kept"));
    custom.insert(
        reqwest::header::AUTHORIZATION,
        HeaderValue::from_static("attacker"),
    );
    custom.insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    let (url, handle) = server(200, "application/json", "{}", "");
    let client = client_for(&url, "real-token", custom);
    let _: serde_json::Value = client
        .request_json(Method::POST, "/test", Some(json!({})))
        .await
        .unwrap();
    let request = handle.join().unwrap();
    assert!(request.contains("authorization: Bearer real-token"));
    assert!(!request.contains("attacker"));
    assert!(request.contains("content-type: application/json"));
    assert!(request.contains("x-custom: kept"));
}

#[tokio::test]
async fn client_maps_all_required_http_statuses_and_retry_after() {
    for status in [401, 403, 404, 405, 422, 429, 500] {
        let (url, handle) = server(status, "text/plain", "failure", "Retry-After: 9\r\n");
        let error = client_for(&url, "token", HeaderMap::new())
            .request_text(Method::GET, "/test")
            .await
            .unwrap_err();
        assert!(matches!(error, CoolifyApiError::Http { status: actual, .. } if actual == status));
        assert_eq!(error.retry_after(), Some("9"));
        handle.join().unwrap();
    }
}

#[tokio::test]
async fn client_bounds_multibyte_text_response() {
    let body = "é".repeat(coolify_api::MAX_BODY_BYTES);
    let (url, handle) = server(200, "text/plain", &body, "");
    let text = client_for(&url, "token", HeaderMap::new())
        .request_text(Method::GET, "/test")
        .await
        .unwrap();
    assert!(text.len() <= coolify_api::MAX_BODY_BYTES);
    assert!(text.is_char_boundary(text.len()));
    handle.join().unwrap();

    let body = format!(
        r#"{{"data":"{}"}}"#,
        "a".repeat(coolify_api::MAX_BODY_BYTES)
    );
    let (url, handle) = server(200, "application/json", &body, "");
    let error = client_for(&url, "token", HeaderMap::new())
        .request_json::<serde_json::Value>(Method::GET, "/test", None)
        .await
        .unwrap_err();
    assert!(matches!(error, CoolifyApiError::Decode(_)));
    handle.join().unwrap();
}
