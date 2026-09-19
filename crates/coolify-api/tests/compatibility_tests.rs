use coolify_api::{api_shape, error_hint};

#[test]
fn routing_miss_is_body_based() {
    assert!(api_shape::is_routing_catch_all(
        404,
        r#"{"message":"Not found."}"#
    ));
    assert!(api_shape::is_routing_catch_all(
        404,
        r#"{"docs":"https://docs"}"#
    ));
    assert!(!api_shape::is_routing_catch_all(
        404,
        r#"{"message":"Resource not found"}"#
    ));
    assert!(!api_shape::is_routing_catch_all(
        500,
        r#"{"message":"Not found."}"#
    ));
}

#[test]
fn known_error_hints_are_stable() {
    assert!(error_hint(500, "/scheduled-tasks").is_some());
    assert!(error_hint(405, "/servers/s/validate").is_some());
    assert!(error_hint(403, "/servers").is_some());
}
