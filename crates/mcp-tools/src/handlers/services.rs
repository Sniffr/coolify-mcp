use serde_json::Value;
pub fn accepts(tool: &str) -> bool {
    matches!(tool, "service" | "list_services" | "get_service")
}
pub fn validate(_args: &Value) -> bool {
    true
}
