use serde_json::Value;
pub fn accepts(tool: &str) -> bool {
    matches!(tool, "diagnose_app" | "diagnose_server" | "find_issues")
}
pub fn validate(_args: &Value) -> bool {
    true
}
