use mcp_tools::{InstanceRegistry, registered_tools};
use safety::CapabilityProfile;
use serde_json::json;

#[test]
fn single_instance_has_no_selector_and_fleet_has_one() {
    let single = registered_tools(
        CapabilityProfile::Operations,
        Some(&InstanceRegistry::new(vec!["one".into()])),
    );
    assert!(!single.iter().any(|t| t.name == "list_instances"));
    assert!(
        single
            .iter()
            .all(|t| !t.input_schema["properties"].get("instance").is_some())
    );

    let fleet = registered_tools(
        CapabilityProfile::Operations,
        Some(&InstanceRegistry::new(vec!["one".into(), "two".into()])),
    );
    assert!(fleet.iter().any(|t| t.name == "list_instances"));
    assert!(
        fleet
            .iter()
            .filter(|t| t.name != "list_instances")
            .all(|t| t.input_schema["properties"]["instance"].is_object())
    );
}

#[test]
fn instances_parse_without_echoing_secrets() {
    let registry = InstanceRegistry::from_json(
        r#"[{"name":"prod","url":"https://prod.example","token":"super-secret"}]"#,
    )
    .unwrap();
    assert_eq!(registry.all(), &["prod"]);
    assert!(!format!("{registry:?}").contains("super-secret"));
    let err =
        InstanceRegistry::from_json(r#"[{"name":"bad","url":"file:///tmp","token":"secret"}]"#)
            .unwrap_err();
    assert!(!err.to_string().contains("secret"));
}

#[test]
fn unknown_instance_is_rejected_before_routing() {
    let registry = InstanceRegistry::new(vec!["one".into(), "two".into()]);
    assert!(!registry.get("missing"));
    assert!(registry.select("missing").is_err());
    assert_eq!(registry.select("two").unwrap().name(), "two");
}

#[test]
fn list_instances_schema_requires_no_instance() {
    let tools = registered_tools(
        CapabilityProfile::ReadOnly,
        Some(&InstanceRegistry::new(vec!["one".into(), "two".into()])),
    );
    let tool = tools.iter().find(|t| t.name == "list_instances").unwrap();
    assert_eq!(tool.input_schema["required"], json!([]));
}
