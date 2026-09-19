use mcp_tools::resources::register_resources;
use mcp_tools::{InstanceRegistry, ToolContext, call_tool};
use safety::CapabilityProfile;
use serde_json::{Value, json};
use std::sync::Arc;

fn context(registry: Option<Arc<InstanceRegistry>>) -> ToolContext {
    let client = registry
        .as_ref()
        .and_then(|r| r.select("one").ok())
        .map(|i| i.client)
        .unwrap_or_else(|| {
            Arc::new(
                coolify_api::CoolifyClient::new(coolify_api::CoolifyConfig {
                    base_url: "http://127.0.0.1".parse().unwrap(),
                    token_source: coolify_api::TokenSource::from_env(
                        &std::collections::HashMap::from([(
                            "COOLIFY_ACCESS_TOKEN".into(),
                            "test".into(),
                        )]),
                    )
                    .unwrap(),
                    custom_headers: Default::default(),
                    timeout: std::time::Duration::from_secs(1),
                })
                .unwrap(),
            )
        });
    ToolContext {
        client,
        policy: CapabilityProfile::ReadOnly,
        audit: None,
        instance: None,
        instance_registry: registry,
        request_metadata: Default::default(),
    }
}

#[tokio::test]
async fn list_instances_requires_fleet_context_and_projects_safe_metadata() {
    let result = call_tool(context(None), "list_instances", json!({})).await;
    let value: Value = serde_json::from_str(&result.text).unwrap();
    assert!(result.is_error);
    assert_eq!(value["error"]["code"], "MCP_UNSUPPORTED");

    let registry = Arc::new(InstanceRegistry::from_json(r#"[{"name":"one","url":"https://one.example/api/","token":"secret-one"},{"name":"two","url":"https://two.example","token":"secret-two"}]"#).unwrap());
    let value: Value = serde_json::from_str(
        &call_tool(context(Some(registry)), "list_instances", json!({}))
            .await
            .text,
    )
    .unwrap();
    let rows = value["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "one");
    assert!(
        rows[0]["base_url"]
            .as_str()
            .unwrap()
            .contains("one.example")
    );
    assert!(!value.to_string().contains("secret-one"));
}

#[tokio::test]
async fn unknown_selected_instance_fails_before_api_request() {
    let registry = Arc::new(InstanceRegistry::from_json(r#"[{"name":"one","url":"http://127.0.0.1:1","token":"secret"},{"name":"two","url":"http://127.0.0.1:2","token":"secret"}]"#).unwrap());
    let result = call_tool(
        context(Some(registry)),
        "list_applications",
        json!({"instance":"missing"}),
    )
    .await;
    assert!(result.is_error);
    assert!(result.text.contains("unknown instance name"));
}

#[tokio::test]
async fn search_docs_uses_embedded_framed_results_without_api() {
    let result = call_tool(
        context(None),
        "search_docs",
        json!({"query":"deployment logs", "per_page": 2}),
    )
    .await;
    assert!(!result.is_error);
    assert!(result.text.contains("BEGIN UNTRUSTED LOG OUTPUT"));
    assert!(result.text.contains("Deployments"));
}

#[test]
fn resources_resolve_concrete_application_uri_and_are_read_only() {
    let resources = register_resources();
    let resolved = resources.resolve("coolify://application/abc").unwrap();
    assert!(resolved.read_only);
    assert_eq!(resolved.uuid.as_deref(), Some("abc"));
    assert!(resources.resolve("coolify://application/").is_none());
}
