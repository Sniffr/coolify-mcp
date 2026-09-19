use serde_json::Value;
pub fn is_routing_catch_all(status: u16, body: &str) -> bool {
    if status != 404 {
        return false;
    }
    serde_json::from_str::<Value>(body).ok().is_some_and(|v| {
        v.get("docs").is_some() || v.get("message").and_then(Value::as_str) == Some("Not found.")
    })
}
pub fn unwrap_logs(value: Value) -> String {
    value
        .get("logs")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
pub fn pagination_query(page: u32, per_page: u32) -> String {
    format!("page={page}&per_page={per_page}")
}
