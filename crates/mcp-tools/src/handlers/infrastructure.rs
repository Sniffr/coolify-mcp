use serde_json::Value;
pub fn accepts(tool: &str) -> bool {
    matches!(
        tool,
        "get_infrastructure_overview"
            | "get_version"
            | "get_mcp_version"
            | "list_servers"
            | "get_server"
            | "server_domains"
            | "server_resources"
            | "list_destinations"
    )
}
pub fn validate(_args: &Value) -> bool {
    true
}
