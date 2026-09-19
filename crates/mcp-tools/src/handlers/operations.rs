use serde_json::Value;
pub fn accepts(tool: &str) -> bool {
    matches!(
        tool,
        "bulk_env_update"
            | "stop_all_apps"
            | "redeploy_project"
            | "restart_project_apps"
            | "control"
    )
}
pub fn validate(_args: &Value) -> bool {
    true
}
