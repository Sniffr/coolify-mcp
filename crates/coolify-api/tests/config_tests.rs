use std::collections::HashMap;

use coolify_api::{ConfigError, config_from_env};

fn env(values: &[(&str, &str)]) -> HashMap<String, String> {
    values
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect()
}

#[test]
fn canonical_url_and_token_win_over_legacy_aliases() {
    let config = config_from_env(
        &env(&[
            ("COOLIFY_BASE_URL", "https://canonical.example///"),
            ("COOLIFY_URL", "https://legacy.example"),
            ("COOLIFY_ACCESS_TOKEN", "canonical-secret"),
            ("COOLIFY_TOKEN", "legacy-secret"),
        ]),
        false,
    )
    .unwrap();
    assert_eq!(config.base_url.as_str(), "https://canonical.example/");
    assert_eq!(config.token_source.current().unwrap(), "canonical-secret");
}

#[test]
fn missing_values_are_named_and_never_leak_values() {
    let error = config_from_env(&HashMap::new(), false).unwrap_err();
    let text = error.to_string();
    assert!(text.contains("COOLIFY_BASE_URL"));
    assert!(matches!(error, ConfigError::MissingUrl));

    let error =
        config_from_env(&env(&[("COOLIFY_BASE_URL", "https://example.test")]), false).unwrap_err();
    assert!(error.to_string().contains("COOLIFY_ACCESS_TOKEN"));
    assert!(!error.to_string().contains("secret"));
}

#[test]
fn token_file_wins_over_both_inline_tokens() {
    let path = std::env::temp_dir().join(format!("coolify-config-token-{}", std::process::id()));
    std::fs::write(&path, "file-token\n").unwrap();
    let config = config_from_env(
        &env(&[
            ("COOLIFY_BASE_URL", "https://example.test"),
            ("COOLIFY_ACCESS_TOKEN_FILE", path.to_str().unwrap()),
            ("COOLIFY_ACCESS_TOKEN", "access-token"),
            ("COOLIFY_TOKEN", "legacy-token"),
        ]),
        false,
    )
    .unwrap();
    assert_eq!(config.token_source.current().unwrap(), "file-token");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn empty_canonical_values_fall_back_to_legacy_values() {
    let config = config_from_env(
        &env(&[
            ("COOLIFY_BASE_URL", "  "),
            ("COOLIFY_URL", "https://legacy.example///"),
            ("COOLIFY_ACCESS_TOKEN", ""),
            ("COOLIFY_TOKEN", "legacy-token"),
        ]),
        false,
    )
    .unwrap();
    assert_eq!(config.base_url.as_str(), "https://legacy.example/");
    assert_eq!(config.token_source.current().unwrap(), "legacy-token");
}

#[test]
fn invalid_url_and_non_http_scheme_are_rejected_without_values() {
    for value in ["not a url", "file:///tmp/coolify"] {
        let error = config_from_env(
            &env(&[
                ("COOLIFY_BASE_URL", value),
                ("COOLIFY_ACCESS_TOKEN", "secret"),
            ]),
            false,
        )
        .unwrap_err();
        assert!(!error.to_string().contains(value));
    }
}
