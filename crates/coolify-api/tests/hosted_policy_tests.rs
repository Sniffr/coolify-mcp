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
    // Use a public literal address so this policy unit test does not depend on
    // external DNS availability. Production validation still resolves hostnames.
    let url = "https://1.1.1.1".parse().unwrap();
    assert!(validate_hosted_base_url(&url).is_ok());
    assert!(CoolifyClient::new_hosted(config("https://1.1.1.1")).is_ok());
}

#[test]
fn hosted_policy_allows_plain_http_to_public_addresses_with_warning() {
    // Plain http is accepted for hosts without TLS (the settings UI warns the
    // token travels unencrypted); DNS pinning and the public-address check
    // still apply. Use literals so this test needs no external DNS.
    let url = "http://1.1.1.1:8000".parse().unwrap();
    assert!(validate_hosted_base_url(&url).is_ok());
    assert!(CoolifyClient::new_hosted(config("http://1.1.1.1:8000")).is_ok());
    // Private/local destinations stay rejected over http too.
    assert!(validate_hosted_base_url(&"http://192.168.1.1".parse().unwrap()).is_err());
    assert!(CoolifyClient::new_hosted(config("http://127.0.0.1")).is_err());
}
