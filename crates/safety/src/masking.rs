use serde_json::Value;

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
    "environment_variables",
    "internal_db_url",
    "docker_compose",
    "custom_labels",
    "connection_url",
    "webhook_url",
];

pub fn sanitize_json(value: &Value, reveal: bool) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    let masked = ALWAYS_MASKED.iter().any(|field| lower == *field)
                        || (!reveal && SENSITIVE.iter().any(|field| lower == *field));
                    (
                        key.clone(),
                        if masked && !value.is_null() {
                            Value::String("***".into())
                        } else {
                            sanitize_json(value, reveal)
                        },
                    )
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
