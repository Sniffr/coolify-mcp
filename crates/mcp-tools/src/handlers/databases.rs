use serde_json::Value;
pub fn accepts(tool: &str) -> bool {
    matches!(
        tool,
        "database" | "database_backups" | "list_databases" | "get_database"
    )
}
pub fn validate(_args: &Value) -> bool {
    true
}
