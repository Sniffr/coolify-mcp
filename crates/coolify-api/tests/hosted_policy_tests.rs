use coolify_api::{CoolifyClient, CoolifyConfig, TokenSource, validate_hosted_base_url};
use reqwest::header::HeaderMap;
use std::time::Duration;

fn config(url: &str) -> CoolifyConfig {
    CoolifyConfig {
        base_url: url.parse().unwrap(),
        token_source: TokenSource::from_env(&std::collections::HashMap::from([(
            "COOLIFY_ACCESS_TOKEN".to_owned(),
            "test-token".to_owned(),
        )]))
        .unwrap(),
        custom_headers: HeaderMap::new(),
        timeout: Duration::from_secs(1),
    }
}

#[test]
fn hosted_policy_rejects_private_and_metadata_destinations() {
    for url in [
        "https://127.0.0.1",
        "https://10.0.0.8",
        "https://172.16.0.1",
        "https://192.168.1.1",
        "https://169.254.169.254",
        "https://[::1]",
        "https://[fc00::1]",
        "https://[fe80::1]",
    ] {
        assert!(
            validate_hosted_base_url(&url.parse().unwrap()).is_err(),
            "{url}"
        );
        assert!(CoolifyClient::new_hosted(config(url)).is_err(), "{url}");
    }
}

#[test]
fn hosted_policy_allows_public_https() {
    let url = "https://example.com".parse().unwrap();
    assert!(validate_hosted_base_url(&url).is_ok());
    assert!(CoolifyClient::new_hosted(config("https://example.com")).is_ok());
}

#[test]
fn hosted_policy_rejects_plain_http_without_local_escape_hatch() {
    assert!(validate_hosted_base_url(&"http://example.com".parse().unwrap()).is_err());
}
