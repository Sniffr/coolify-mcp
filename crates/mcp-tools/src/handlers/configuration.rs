use serde_json::Value;
pub fn accepts(tool: &str) -> bool {
    matches!(
        tool,
        "cloud_tokens"
            | "private_keys"
            | "github_apps"
            | "hetzner"
            | "scheduled_tasks"
            | "teams"
            | "system"
            | "projects"
            | "environments"
    )
}
pub fn validate(_args: &Value) -> bool {
    true
}
