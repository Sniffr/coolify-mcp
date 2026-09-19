use mcp_tools::schemas::schema_for;
use mcp_tools::{InstanceRegistry, ToolContext, call_tool};
use safety::CapabilityProfile;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[test]
fn schemas_are_closed_and_group_actions_are_enum_constrained() {
    for name in [
        "application",
        "database",
        "service",
        "deployment",
        "projects",
        "control",
        "system",
    ] {
        let schema = schema_for(name, false);
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema["properties"]["action"]["enum"].is_array());
    }
    assert_eq!(
        schema_for("get_version", false)["additionalProperties"],
        false
    );
}

#[test]
fn fleet_urls_reject_credential_query_and_fragment_components() {
    for (url, secret) in [
        ("https://user:password@example.test", "password"),
        ("https://example.test/?token=query-secret", "query-secret"),
        ("https://example.test/#fragment-secret", "fragment-secret"),
    ] {
        let error = InstanceRegistry::from_json(&format!(
            r#"[{{"name":"prod","url":"{url}","token":"api-secret"}}]"#
        ))
        .unwrap_err();
        assert!(!error.to_string().contains(secret));
        assert!(!error.to_string().contains("api-secret"));
    }
}

#[test]
fn duplicate_names_are_rejected_without_default_ambiguity() {
    let error = InstanceRegistry::from_json(r#"[{"name":"prod","url":"https://one.example","token":"one"},{"name":"prod","url":"https://two.example","token":"two"}]"#).unwrap_err();
    assert!(error.to_string().contains("duplicate instance name"));
    assert!(!error.to_string().contains("one"));
    assert!(!error.to_string().contains("two"));
}

fn server(counter: Arc<AtomicUsize>, marker: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        counter.fetch_add(1, Ordering::SeqCst);
        let mut request = [0_u8; 2048];
        let size = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..size]);
        assert!(request.contains("/api/v1/applications"));
        let body = format!(r#"[{{"uuid":"{marker}-uuid","name":"{marker}"}}]"#);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    format!("http://{}", address)
}

fn context(registry: Arc<InstanceRegistry>) -> ToolContext {
    ToolContext {
        client: registry.select("one").unwrap().client,
        policy: CapabilityProfile::ReadOnly,
        audit: None,
        instance: None,
        instance_registry: Some(registry),
        request_metadata: Default::default(),
    }
}

#[tokio::test]
async fn concurrent_selected_calls_reach_only_their_own_clients() {
    let one_count = Arc::new(AtomicUsize::new(0));
    let two_count = Arc::new(AtomicUsize::new(0));
    let one_url = server(one_count.clone(), "one");
    let two_url = server(two_count.clone(), "two");
    let registry = Arc::new(InstanceRegistry::from_json(&format!(r#"[{{"name":"one","url":"{one_url}","token":"one-token"}},{{"name":"two","url":"{two_url}","token":"two-token"}}]"#)).unwrap());
    let first = call_tool(
        context(registry.clone()),
        "list_applications",
        json!({"instance":"one"}),
    );
    let second = call_tool(
        context(registry),
        "list_applications",
        json!({"instance":"two"}),
    );
    let (first, second) = tokio::join!(first, second);
    assert!(!first.is_error, "{}", first.text);
    assert!(!second.is_error, "{}", second.text);
    assert_eq!(one_count.load(Ordering::SeqCst), 1);
    assert_eq!(two_count.load(Ordering::SeqCst), 1);
    assert!(first.text.contains("one"));
    assert!(second.text.contains("two"));
}
