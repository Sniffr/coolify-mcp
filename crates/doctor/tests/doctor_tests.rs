use doctor::{DoctorCheckStatus, DoctorReport, run_doctor};
use std::collections::HashMap;

#[tokio::test]
async fn reports_missing_configuration_without_echoing_values() {
    let env = HashMap::new();
    let report = run_doctor(&env, |_path| async { Ok::<_, String>("{}".into()) }).await;
    assert!(!report.ok);
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.name == "coolify_url" && c.status == DoctorCheckStatus::Fail)
    );
    let rendered = serde_json::to_string(&report).unwrap();
    assert!(!rendered.contains("secret"));
}

#[tokio::test]
async fn detects_literal_variables_and_doubled_api_path() {
    let mut env = HashMap::new();
    env.insert("COOLIFY_URL".into(), "${COOLIFY_URL}/api/v1".into());
    env.insert("COOLIFY_TOKEN".into(), "${COOLIFY_TOKEN}".into());
    let report = run_doctor(&env, |_path| async { Ok::<_, String>("{}".into()) }).await;
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.name == "literal_variables" && c.status == DoctorCheckStatus::Fail)
    );
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.name == "api_path" && c.status == DoctorCheckStatus::Fail)
    );
}

#[tokio::test]
async fn emits_clean_json_and_checks_version_routing_and_access() {
    let mut env = HashMap::new();
    env.insert("COOLIFY_BASE_URL".into(), "https://coolify.example".into());
    env.insert("COOLIFY_ACCESS_TOKEN".into(), "top-secret-token".into());
    let report = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            match path.as_str() {
                "/version" => Ok("4.2.1".into()),
                "/servers" => Ok("[]".into()),
                "/applications" => Err("401 unauthorized top-secret-token".into()),
                _ => Ok("{}".into()),
            }
        }
    })
    .await;
    assert!(report.checks.iter().any(|c| c.name == "coolify_reachable"));
    assert!(report.checks.iter().any(|c| c.name == "version"));
    assert!(report.checks.iter().any(|c| c.name == "routing_catch_all"));
    let json = report.to_json().unwrap();
    assert!(!json.contains("top-secret-token"));
}

#[test]
fn human_report_has_fix_guidance() {
    let report = DoctorReport {
        ok: false,
        checks: vec![],
    };
    assert!(report.to_human().contains("doctor"));
}
