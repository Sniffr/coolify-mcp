use serde_json::Value;
pub fn accepts(tool: &str) -> bool {
    tool == "search_docs"
}
pub fn validate(_args: &Value) -> bool {
    true
}
