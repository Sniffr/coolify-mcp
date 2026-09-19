use serde_json::Value;
pub fn accepts(tool: &str) -> bool {
    matches!(tool, "deploy" | "deployment" | "list_deployments" | "logs")
}
pub fn validate(_args: &Value) -> bool {
    true
}
