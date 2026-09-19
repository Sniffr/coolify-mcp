use mcp_tools::schemas::schema_for;

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
