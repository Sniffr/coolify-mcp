use mcp_tools::{DEFAULT_TOOL_ROSTER, InstanceRegistry, registered_tools};
use safety::CapabilityProfile;

#[test]
fn default_roster_is_exact_and_unique() {
    assert_eq!(DEFAULT_TOOL_ROSTER.len(), 45);
    let tools = registered_tools(CapabilityProfile::Operations, None);
    assert_eq!(tools.len(), 45);
    let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
    let expected: Vec<_> = DEFAULT_TOOL_ROSTER
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, expected);
    for tool in tools {
        assert!(!tool.title.is_empty());
        assert!(tool.input_schema.is_object());
    }
}
#[test]
fn readonly_omits_writes_and_fleet_adds_only_selector() {
    let read = registered_tools(CapabilityProfile::ReadOnly, None);
    assert!(read.iter().all(|t| t.annotations.read_only_hint));
    let fleet = registered_tools(
        CapabilityProfile::Operations,
        Some(&InstanceRegistry::new(vec!["a".into(), "b".into()])),
    );
    assert_eq!(fleet.len(), 45);
    assert!(!fleet.iter().any(|tool| tool.name == "list_instances"));
}
