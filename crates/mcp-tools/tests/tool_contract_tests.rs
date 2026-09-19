use mcp_tools::{DEFAULT_TOOL_ROSTER, registered_tools};
use safety::CapabilityProfile;

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
