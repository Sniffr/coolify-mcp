use serde_json::Value;
pub fn accepts(tool: &str) -> bool {
    matches!(
        tool,
        "application"
            | "application_logs"
            | "get_application"
            | "list_applications"
            | "env_vars"
            | "storages"
            | "tags"
            | "control"
    )
}
pub fn validate(_args: &Value) -> bool {
    true
}
