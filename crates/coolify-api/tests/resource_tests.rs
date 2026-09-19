use coolify_api::{
    ApplicationSummary, DatabaseSummary, DeploymentSummary, ServerSummary, is_running_status,
};
use serde_json::json;

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
}

#[test]
fn logs_and_deployment_projection_are_bounded() {
    assert_eq!(coolify_api::unwrap_logs(json!({"logs":"hello"})), "hello");
    assert_eq!(coolify_api::unwrap_logs(json!("bare")), "bare");
    let deployment: DeploymentSummary = serde_json::from_value(json!({"uuid":"d","deployment_uuid":"dep","status":"running","created_at":"now","raw":"hidden"})).unwrap();
    assert_eq!(deployment.deployment_uuid, "dep");
    assert!(is_running_status(Some("running:unhealthy")));
    assert!(!is_running_status(Some("exited:unhealthy")));
}

#[test]
fn pagination_is_encoded() {
    assert_eq!(coolify_api::pagination_query(2, 50), "page=2&per_page=50");
}
