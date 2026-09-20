use coolify_api::{
    ApplicationSummary, CoolifyClient, DatabaseSummary, DeploymentSummary, ServerSummary,
    ValidationResult, config_from_env, is_running_status,
};
use serde_json::json;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn application_list_uses_encoded_pagination_and_summary_projection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(String::new()));
    let out = Arc::clone(&seen);
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        *out.lock().unwrap() = String::from_utf8_lossy(&buf[..n]).to_string();
        let body =
            r#"[{"uuid":"a","name":"web","domains":["https://example.test"],"secret":"masked"}]"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let mut env = HashMap::new();
    env.insert("COOLIFY_BASE_URL".into(), format!("http://{addr}"));
    env.insert("COOLIFY_ACCESS_TOKEN".into(), "token".into());
    let client = CoolifyClient::new(config_from_env(&env, false).unwrap()).unwrap();
    let apps = client.list_applications(2, 50).await.unwrap();
    assert_eq!(apps[0].fqdn.as_deref(), Some("https://example.test"));
    let request = seen.lock().unwrap().clone();
    assert!(request.starts_with("GET /api/v1/applications?page=2&per_page=50"));
}

#[test]
fn validation_projection_discards_unknown_fields() {
    let result: ValidationResult=serde_json::from_value(json!({"valid":true,"status":"ok","message":"ready","version":"4.0","capabilities":["deploy"],"secret":"discard"})).unwrap();
    assert!(result.valid);
    assert_eq!(result.status.as_deref(), Some("ok"));
    assert_eq!(result.capabilities, vec!["deploy"]);
}

#[test]
fn projects_and_servers_are_bounded_summaries() {
    let server: ServerSummary = serde_json::from_value(
        json!({"uuid":"s1","name":"edge","ip":"10.0.0.1","status":"running","secret":"no"}),
    )
    .unwrap();
    assert_eq!(server.uuid, "s1");
    assert!(server.extra.as_object().is_some_and(|v| v.is_empty()));
}

#[test]
fn application_domain_and_database_type_are_compatible() {
    let app: ApplicationSummary = serde_json::from_value(json!({"uuid":"a1","name":"web","domains":["https://example.test"],"git_repository":"x","git_branch":"main"})).unwrap();
    assert_eq!(app.fqdn.as_deref(), Some("https://example.test"));
    let db: DatabaseSummary = serde_json::from_value(json!({"uuid":"d1","name":"db","database_type":"postgresql","status":"running","is_public":false,"environment_id":7})).unwrap();
    assert_eq!(db.r#type, "postgresql");
    assert_eq!(db.environment_id, Some(7));
    let service: coolify_api::ServiceSummary = serde_json::from_value(
        json!({"uuid":"s","name":"svc","domains":[{"fqdn":"https://svc.test"}, {"other":true}]}),
    )
    .unwrap();
    assert_eq!(service.domains, Some(vec!["https://svc.test".to_string()]));
    let unknown: coolify_api::ServiceSummary =
        serde_json::from_value(json!({"uuid":"s","name":"svc","domains":{"unexpected":true}}))
            .unwrap();
    assert_eq!(unknown.domains, Some(Vec::new()));
}

#[test]
fn logs_and_deployment_projection_are_bounded() {
    assert_eq!(coolify_api::unwrap_logs(json!({"logs":"hello"})), "hello");
    assert_eq!(coolify_api::unwrap_logs(json!("bare")), "bare");
    let deployment: DeploymentSummary = serde_json::from_value(json!({"uuid":"d","deployment_uuid":"dep","status":"running","created_at":"now","raw":"hidden"})).unwrap();
    assert_eq!(deployment.deployment_uuid, "dep");
    assert!(!is_running_status(Some("running:unhealthy")));
    assert!(!is_running_status(Some("exited:unhealthy")));
    assert!(is_running_status(Some("running")));
}

#[test]
fn pagination_is_encoded() {
    assert_eq!(coolify_api::pagination_query(2, 50), "page=2&per_page=50");
}

#[test]
fn application_list_accepts_pagination_envelopes_and_id_fallback() {
    // Bare array (existing behavior).
    let bare: Vec<ApplicationSummary> =
        serde_json::from_value(json!([{"uuid":"a1","name":"web"}])).unwrap();
    assert_eq!(bare[0].uuid, "a1");

    // Real Coolify versions wrap lists in a Laravel pagination envelope.
    // request_list normalizes these; the item parsing must also tolerate
    // numeric `id` without `uuid`.
    let envelope_item: ApplicationSummary =
        serde_json::from_value(json!({"id":7,"name":"web"})).unwrap();
    assert_eq!(envelope_item.uuid, "7");
    assert_eq!(envelope_item.name, "web");
}

#[tokio::test]
async fn application_list_accepts_bodies_larger_than_error_bound() {
    // A single Coolify application object can exceed the 10 KiB error-body
    // bound; the success path must not truncate it mid-string.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        let big_name = "w".repeat(20_000);
        let body = format!(r#"[{{"uuid":"a","name":"{big_name}"}}]"#);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let mut env = HashMap::new();
    env.insert("COOLIFY_BASE_URL".into(), format!("http://{addr}"));
    env.insert("COOLIFY_ACCESS_TOKEN".into(), "token".into());
    let client = CoolifyClient::new(config_from_env(&env, false).unwrap()).unwrap();
    let apps = client.list_applications(1, 50).await.unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name.len(), 20_000);
}

#[tokio::test]
async fn application_list_accepts_data_envelope_over_http() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        let body = r#"{"data":[{"uuid":"a","name":"web"}],"meta":{"total":1}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let mut env = HashMap::new();
    env.insert("COOLIFY_BASE_URL".into(), format!("http://{addr}"));
    env.insert("COOLIFY_ACCESS_TOKEN".into(), "token".into());
    let client = CoolifyClient::new(config_from_env(&env, false).unwrap()).unwrap();
    let apps = client.list_applications(1, 50).await.unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].uuid, "a");
}
