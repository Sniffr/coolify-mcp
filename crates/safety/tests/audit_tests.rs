use safety::{AuditEvent, AuditLogger};
use serde_json::Value;
use std::io::Cursor;

fn event(timestamp: &str, tool: &str) -> AuditEvent {
    AuditEvent::new(
        timestamp,
        Some("client-1"),
        tool,
        "resource-7",
        "success",
        200,
        12,
    )
}

#[test]
fn audit_chain_links_events_and_hash_changes_with_event_fields() {
    let mut output = Cursor::new(Vec::new());
    let mut logger = AuditLogger::new(&mut output);
    let first = logger
        .record(event("2026-01-01T00:00:00Z", "list_apps"))
        .unwrap();
    let second = logger
        .record(event("2026-01-01T00:00:01Z", "list_apps"))
        .unwrap();
    assert_ne!(first, second);
    let lines = String::from_utf8(output.into_inner()).unwrap();
    let rows: Vec<Value> = lines
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["previous_hash"], "");
    assert_eq!(rows[1]["previous_hash"], rows[0]["hash"]);
    assert!(rows[0].get("request").is_none());
    assert!(rows[0].get("response").is_none());
}

#[test]
fn audit_output_contains_identifiers_but_not_secret_values() {
    let mut output = Cursor::new(Vec::new());
    let mut logger = AuditLogger::new(&mut output);
    logger
        .record(AuditEvent::new(
            "now",
            Some("client"),
            "deploy",
            "app-1",
            "failure",
            500,
            1,
        ))
        .unwrap();
    let line = String::from_utf8(output.into_inner()).unwrap();
    assert!(line.contains("deploy"));
    assert!(line.contains("app-1"));
    assert!(!line.contains("password"));
    assert!(!line.contains("token"));
}
