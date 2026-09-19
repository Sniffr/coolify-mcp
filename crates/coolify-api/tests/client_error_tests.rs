use coolify_api::{CoolifyApiError, HttpErrorDetails};

#[test]
fn http_errors_retain_status_retry_after_and_bounded_body() {
    let details = HttpErrorDetails::new(429, "x".repeat(20_000), Some("7".into()));
    let error = CoolifyApiError::http(details);
    let text = error.to_string();
    assert!(text.contains("429"));
    assert!(text.contains("Retry-After"));
    assert!(text.len() < 12_000);
    assert!(error.retry_after() == Some("7"));
}

#[test]
fn errors_do_not_debug_or_display_secrets() {
    let error = CoolifyApiError::config("bad token configuration");
    assert!(!format!("{error:?}").contains("secret"));
    assert!(!format!("{error}").contains("secret"));
}
