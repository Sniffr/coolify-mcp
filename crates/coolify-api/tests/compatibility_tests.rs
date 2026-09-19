use coolify_api::route_matrix::{ROUTE_MATRIX, SafetyClass};
use coolify_api::{
    CoolifyClient, LegacyEndpoint, api_shape, config_from_env, error_hint, error_hint_with_body,
};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

fn fake_server(responses: Vec<(u16, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let out = Arc::clone(&seen);
    std::thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            out.lock()
                .unwrap()
                .push(request.lines().next().unwrap_or_default().to_string());
            let reason = if status == 200 { "OK" } else { "ERR" };
            write!(stream,"HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",body.len(),body).unwrap();
        }
    });
    (format!("http://{}", addr), seen)
}

fn client_at(base: String) -> CoolifyClient {
    let mut env = HashMap::new();
    env.insert("COOLIFY_BASE_URL".into(), base);
    env.insert("COOLIFY_ACCESS_TOKEN".into(), "token".into());
    CoolifyClient::new(config_from_env(&env, false).unwrap()).unwrap()
}

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

#[tokio::test]
async fn fallback_caches_get_and_reprobes_after_cached_405() {
    let (base, seen) = fake_server(vec![
        (405, r#"{"message":"method"}"#),
        (200, r#"{"ok":true}"#),
        (405, r#"{"message":"method"}"#),
        (200, r#"{"ok":true}"#),
        (200, r#"{"ok":true}"#),
    ]);
    let client = client_at(base);
    let _: serde_json::Value = client
        .post_with_legacy_get_fallback(LegacyEndpoint::ServersValidate, "/servers/x/validate", None)
        .await
        .unwrap();
    let _: serde_json::Value = client
        .post_with_legacy_get_fallback(LegacyEndpoint::ServersValidate, "/servers/x/validate", None)
        .await
        .unwrap();
    let requests = seen.lock().unwrap().clone();
    assert!(requests[0].starts_with("POST /api/v1/servers/x/validate"));
    assert!(requests[1].starts_with("GET /api/v1/servers/x/validate"));
    assert!(requests[2].starts_with("GET /api/v1/servers/x/validate"));
    assert!(requests[3].starts_with("POST /api/v1/servers/x/validate"));
}

#[tokio::test]
async fn deployment_poll_stops_on_terminal_projection() {
    let body1 = r#"{"uuid":"d","deployment_uuid":"dep","status":"running","created_at":"now"}"#;
    let body2 = r#"{"uuid":"d","deployment_uuid":"dep","status":"finished","created_at":"now"}"#;
    let (base, seen) = fake_server(vec![(200, body1), (200, body2)]);
    let client = client_at(base);
    let result = client
        .poll_deployment(
            "d",
            std::time::Duration::from_millis(1),
            std::time::Duration::from_secs(1),
        )
        .await
        .unwrap();
    assert_eq!(result.status, "finished");
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn controller_errors_do_not_retry_mutation() {
    for status in [404, 422, 500] {
        let (base, seen) = fake_server(vec![(status, r#"{"message":"controller"}"#)]);
        let client = client_at(base);
        let result: Result<serde_json::Value, _> = client
            .post_with_legacy_get_fallback(
                LegacyEndpoint::ServersValidate,
                "/servers/x/validate",
                None,
            )
            .await;
        assert!(result.is_err());
        assert_eq!(seen.lock().unwrap().len(), 1);
    }
}

#[test]
fn route_matrix_has_unique_explicit_contracts() {
    let mut keys = std::collections::HashSet::new();
    for route in ROUTE_MATRIX {
        assert!(keys.insert((route.group, route.action)));
        assert!(!route.method.contains('|'));
        assert!(!route.path.contains(" and "));
        assert!(!route.projection.is_empty());
        assert!(!route.compatibility.is_empty());
        let placeholders = route
            .path
            .split('{')
            .skip(1)
            .map(|part| part.split('}').next().unwrap());
        for placeholder in placeholders {
            assert!(
                route.required_args.contains(&placeholder),
                "missing required arg {placeholder} for {}",
                route.path
            );
        }
        if route.safety == SafetyClass::Read {
            assert!(
                !route.method.contains("POST")
                    && !route.method.contains("PATCH")
                    && !route.method.contains("DELETE")
            );
        }
    }
    assert!(
        ROUTE_MATRIX
            .iter()
            .any(|r| r.action == "application_logs" && r.path == "/applications/{uuid}/logs")
    );
    assert!(
        ROUTE_MATRIX
            .iter()
            .any(|r| r.action == "deployment_cancel" && r.required_args == ["uuid"])
    );
    let expected = [
        ("list_servers", "GET", "/servers", "ServerSummary[]"),
        (
            "validate_server",
            "POST",
            "/servers/{uuid}/validate",
            "ValidationResult",
        ),
        (
            "application_logs",
            "GET",
            "/applications/{uuid}/logs",
            "BoundedLogs",
        ),
        (
            "database_logs",
            "GET",
            "/databases/{uuid}/logs",
            "BoundedLogs",
        ),
        (
            "service_children",
            "GET",
            "/services/{uuid}/applications",
            "ActionResult",
        ),
        (
            "deployment_trigger",
            "POST",
            "/deploy",
            "DeploymentProjection",
        ),
        (
            "deployment_cancel",
            "POST",
            "/deployments/{uuid}/cancel",
            "ActionResult",
        ),
        ("system_get", "GET", "/system", "SystemSummary"),
    ];
    for (action, method, path, projection) in expected {
        let route = ROUTE_MATRIX
            .iter()
            .find(|r| r.action == action)
            .expect(action);
        assert_eq!(
            (route.method, route.path, route.projection),
            (method, path, projection)
        );
    }
}

#[test]
fn known_error_hints_are_stable() {
    assert!(error_hint(500, "/scheduled-tasks").is_some());
    assert!(error_hint(405, "/servers/s/validate").is_some());
    assert!(error_hint(403, "/servers").is_some());
    assert!(
        error_hint_with_body(500, "/scheduled-tasks", "command exceeds 255 characters").is_some()
    );
    assert!(error_hint_with_body(404, "/applications/x", "Resource not found").is_some());
    assert!(error_hint_with_body(404, "/applications/x", "Not found.").is_none());
}

#[tokio::test]
async fn deployment_operations_use_typed_paths_and_projections() {
    let item = r#"[{"uuid":"d","deployment_uuid":"dep","status":"finished"}]"#;
    let one = r#"{"uuid":"d","deployment_uuid":"dep","status":"finished"}"#;
    let (base, seen) = fake_server(vec![(200, item), (200, r#"{"logs":"tail"}"#), (200, one)]);
    let client = client_at(base);
    assert_eq!(
        client.list_deployments().await.unwrap()[0].deployment_uuid,
        "dep"
    );
    assert_eq!(client.deployment_logs("d").await.unwrap(), "tail");
    assert_eq!(
        client
            .cancel_deployment("d")
            .await
            .unwrap()
            .status
            .as_deref(),
        Some("finished")
    );
    let requests = seen.lock().unwrap().clone();
    assert!(requests[0].starts_with("GET /api/v1/deployments"));
    assert!(requests[1].starts_with("GET /api/v1/deployments/d/logs"));
    assert!(requests[2].starts_with("POST /api/v1/deployments/d/cancel"));
}

#[tokio::test]
async fn child_operation_segments_are_encoded() {
    let (base, seen) = fake_server(vec![
        (200, r#"{"status":"ok"}"#),
        (200, r#"{"status":"ok"}"#),
    ]);
    let client = client_at(base);
    let _ = client
        .application_storage("a/b ?#", Some("s/t ?#"), reqwest::Method::GET, None)
        .await
        .unwrap();
    let _ = client
        .application_tags("a/b ?#", Some("t/u ?#"), reqwest::Method::GET, None)
        .await
        .unwrap();
    let requests = seen.lock().unwrap().clone();
    assert!(requests[0].contains("/applications/a%2Fb%20%3F%23/storages/s%2Ft%20%3F%23"));
    assert!(requests[1].contains("/applications/a%2Fb%20%3F%23/tags/t%2Fu%20%3F%23"));
}

#[tokio::test]
async fn resource_segments_are_encoded() {
    let (base, seen) = fake_server(vec![(200, r#"{"uuid":"x","name":"safe"}"#)]);
    let client = client_at(base);
    let _ = client.get_application("a/b ?#").await.unwrap();
    let request = seen.lock().unwrap()[0].clone();
    assert!(request.contains("/applications/a%2Fb%20%3F%23"));
    assert!(!request.contains("/applications/a/b"));
}

#[tokio::test]
async fn client_error_exposes_body_derived_hint() {
    let (base, _) = fake_server(vec![(
        500,
        r#"{"message":"scheduled command exceeds 255 characters"}"#,
    )]);
    let client = client_at(base);
    let error = client
        .request_json::<serde_json::Value>(reqwest::Method::GET, "/scheduled-tasks", None)
        .await
        .unwrap_err();
    assert!(error.hint().is_some());
}
