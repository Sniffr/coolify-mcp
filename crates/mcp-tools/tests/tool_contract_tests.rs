use mcp_tools::{
    DEFAULT_TOOL_ROSTER, McpApplication, TenantRequestContext, TenantToolContext,
    call_tool_for_tenant, registered_tools,
};
use safety::CapabilityProfile;
use std::sync::Arc;

fn fixture_client() -> Arc<coolify_api::CoolifyClient> {
    Arc::new(
        coolify_api::CoolifyClient::new(coolify_api::CoolifyConfig {
            base_url: "http://127.0.0.1".parse().unwrap(),
            token_source: coolify_api::TokenSource::from_env(&std::collections::HashMap::from([(
                "COOLIFY_ACCESS_TOKEN".into(),
                "fixture-token".into(),
            )]))
            .unwrap(),
            custom_headers: Default::default(),
            timeout: std::time::Duration::from_secs(1),
        })
        .unwrap(),
    )
}

#[test]
fn every_default_tool_has_complete_contract() {
    let tools = registered_tools(CapabilityProfile::Operations, None);
    assert_eq!(tools.len(), 45);
    for (spec, expected) in tools.iter().zip(DEFAULT_TOOL_ROSTER.iter()) {
        assert_eq!(spec.name, expected.name);
        assert!(!spec.title.is_empty());
        assert!(!spec.description.is_empty());
        assert_eq!(spec.input_schema["type"], "object");
        assert!(!spec.safety.is_empty());
        assert!(
            spec.annotations.read_only_hint
                || spec.annotations.destructive_hint
                || spec.name == "hetzner"
                || spec.name == "validate_server"
        );
    }
}

#[test]
fn read_only_registration_contains_only_reference_reads() {
    let names: Vec<_> = registered_tools(CapabilityProfile::ReadOnly, None)
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(
        names,
        vec![
            "application_logs",
            "diagnose_app",
            "diagnose_server",
            "find_issues",
            "get_application",
            "get_database",
            "get_infrastructure_overview",
            "get_mcp_version",
            "get_server",
            "get_service",
            "get_version",
            "list_applications",
            "list_databases",
            "list_deployments",
            "list_destinations",
            "list_servers",
            "list_services",
            "logs",
            "search_docs",
            "server_domains",
            "server_resources",
            "teams"
        ]
    );
}

#[test]
fn tenant_request_context_keeps_authenticated_user_and_stored_profile() {
    let user_id = tenant::UserId::parse("00000000-0000-0000-0000-000000000001").unwrap();
    let context = TenantRequestContext {
        user_id,
        profile: CapabilityProfile::ReadOnly,
    };
    assert_eq!(context.user_id, user_id);
    assert_eq!(context.profile, CapabilityProfile::ReadOnly);
}

struct LocalOnlyApplication;
impl mcp_tools::McpApplication for LocalOnlyApplication {
    fn tools(&self) -> Vec<mcp_tools::ToolSpec> {
        Vec::new()
    }

    fn call<'a>(
        &'a self,
        _: &'a str,
        _: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = mcp_tools::ToolResult> + Send + 'a>>
    {
        Box::pin(async {
            mcp_tools::ToolResult {
                text: "global client must not be used".into(),
                is_error: false,
            }
        })
    }
}

#[tokio::test]
async fn hosted_dispatch_without_tenant_override_fails_closed() {
    let user_id = tenant::UserId::parse("00000000-0000-0000-0000-000000000001").unwrap();
    let result = LocalOnlyApplication
        .call_for_user(
            TenantToolContext {
                request: TenantRequestContext {
                    user_id,
                    profile: CapabilityProfile::ReadOnly,
                },
                client: fixture_client(),
                audit: None,
                instance_registry: None,
                request_metadata: Default::default(),
            },
            "list_applications",
            serde_json::json!({}),
        )
        .await;
    assert!(result.is_error);
    assert!(!result.text.contains("global client"));
}

#[tokio::test]
async fn stored_read_only_profile_rejects_write_even_when_arguments_request_admin() {
    let user_id = tenant::UserId::parse("00000000-0000-0000-0000-000000000001").unwrap();
    let result = call_tool_for_tenant(
        TenantToolContext {
            request: TenantRequestContext {
                user_id,
                profile: CapabilityProfile::ReadOnly,
            },
            client: fixture_client(),
            audit: None,
            instance_registry: None,
            request_metadata: Default::default(),
        },
        "application",
        serde_json::json!({"uuid":"app-1","action":"delete","profile":"admin"}),
    )
    .await;
    assert!(result.is_error);
    assert!(!result.text.contains("fixture-token"));
}
