use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

const ALWAYS_MASKED: &[&str] = &[
    "password",
    "private_key",
    "webhook_secret",
    "client_secret",
    "api_secret",
    "log_drain_password",
];
const SENSITIVE: &[&str] = &[
    "value",
    "real_value",
    "internal_db_url",
    "docker_compose",
    "custom_labels",
    "connection_url",
    "webhook_url",
];
const TEXT_SENSITIVE: &str = r#"(?i)\b(password|private_key|webhook_secret|client_secret|api_secret|internal_db_url|connection_url|webhook_url|docker_compose|custom_labels|value)\s*([:=])\s*("[^"]*"|'[^']*'|[^\s,;&}]+)"#;

pub fn sanitize_json(value: &Value, reveal: bool) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    let is_environment = lower == "environment_variables";
                    let masked = ALWAYS_MASKED.iter().any(|field| lower == *field)
                        || (!reveal && SENSITIVE.iter().any(|field| lower == *field));
                    let sanitized = if masked && !value.is_null() {
                        Value::String("***".into())
                    } else if is_environment {
                        sanitize_environment(value, reveal)
                    } else {
                        sanitize_json(value, reveal)
                    };
                    (key.clone(), sanitized)
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| sanitize_json(value, reveal))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn sanitize_environment(value: &Value, reveal: bool) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    let secret_value = !reveal && (lower == "value" || lower == "real_value");
                    (
                        key.clone(),
                        if secret_value && !value.is_null() {
                            Value::String("***".into())
                        } else {
                            sanitize_environment(value, reveal)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| sanitize_environment(value, reveal))
                .collect(),
        ),
        _ => value.clone(),
    }
}

pub fn sanitize_text(text: &str) -> String {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(TEXT_SENSITIVE).expect("valid sensitive text pattern"))
        .replace_all(text, |captures: &regex::Captures<'_>| {
            format!("{}{}***", &captures[1], &captures[2])
        })
        .into_owned()
}
