use doctor::{DoctorCheckStatus, DoctorReport, ProbeResponse, run_doctor};
use std::collections::HashMap;

fn response(status: u16, content_type: &str, body: &str) -> ProbeResponse {
    ProbeResponse {
        status,
        content_type: Some(content_type.into()),
        body: body.into(),
        redirected: false,
    }
}

#[tokio::test]
async fn reports_missing_configuration_without_echoing_values() {
    let env = HashMap::new();
    let report = run_doctor(&env, |_path| async {
        Ok::<_, String>(response(200, "application/json", "{}"))
    })
    .await;
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
async fn token_file_satisfies_configuration_without_reading_or_echoing_it() {
    let mut env = HashMap::new();
    env.insert("COOLIFY_URL".into(), "https://coolify.example".into());
    env.insert(
        "COOLIFY_ACCESS_TOKEN_FILE".into(),
        "/private/token-file".into(),
    );
    let report = run_doctor(&env, |_path| async {
        Err::<ProbeResponse, _>("unreachable".into())
    })
    .await;
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "coolify_token")
            .unwrap()
            .status,
        DoctorCheckStatus::Pass
    );
    assert!(!report.to_json().unwrap().contains("/private/token-file"));
}

#[tokio::test]
async fn detects_literal_variables_and_doubled_api_path() {
    let mut env = HashMap::new();
    env.insert("COOLIFY_URL".into(), "${COOLIFY_URL}/api/v1".into());
    env.insert("COOLIFY_TOKEN".into(), "${COOLIFY_TOKEN}".into());
    let report = run_doctor(&env, |_path| async {
        Ok::<_, String>(response(200, "application/json", "{}"))
    })
    .await;
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
async fn classifies_json_api_error_as_valid_routing_but_html_and_redirect_as_catch_all() {
    let mut env = HashMap::new();
    env.insert("COOLIFY_BASE_URL".into(), "https://coolify.example".into());
    env.insert("COOLIFY_ACCESS_TOKEN".into(), "redacted-token".into());
    let report = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            match path.as_str() {
                "/version" => Ok(response(200, "text/plain", "4.2.1")),
                "/__coolify_mcp_doctor_invalid__" => Ok(response(
                    404,
                    "application/json",
                    "{\"message\":\"not found\"}",
                )),
                "/permissions" => Ok(response(200, "application/json", "{\"deploy\":true}")),
                _ => Err("unreachable".into()),
            }
        }
    })
    .await;
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "routing_catch_all")
            .unwrap()
            .status,
        DoctorCheckStatus::Pass
    );

    let html = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            if path == "/__coolify_mcp_doctor_invalid__" {
                Ok(response(200, "text/html", "<html>Cloudflare</html>"))
            } else {
                Ok(response(200, "application/json", "4.2.1"))
            }
        }
    })
    .await;
    assert_eq!(
        html.checks
            .iter()
            .find(|c| c.name == "routing_catch_all")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );

    let redirect = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            if path == "/__coolify_mcp_doctor_invalid__" {
                Ok(ProbeResponse {
                    status: 302,
                    content_type: Some("text/html".into()),
                    body: "location: /login".into(),
                    redirected: true,
                })
            } else {
                Ok(response(200, "text/plain", "4.2.1"))
            }
        }
    })
    .await;
    assert_eq!(
        redirect
            .checks
            .iter()
            .find(|c| c.name == "routing_catch_all")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );
}

#[tokio::test]
async fn checks_invalid_token_unreachable_version_and_missing_deploy_ability() {
    let mut env = HashMap::new();
    env.insert("COOLIFY_URL".into(), "https://coolify.example".into());
    env.insert("COOLIFY_TOKEN".into(), "token-value".into());
    env.insert("MCP_TRANSPORT".into(), "http".into());
    let report = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            match path.as_str() {
                "/version" => Err("401 Unauthorized".into()),
                _ => Err("connection refused".into()),
            }
        }
    })
    .await;
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "token_valid")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "coolify_reachable")
            .unwrap()
            .status,
        DoctorCheckStatus::Inconclusive
    );
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "deploy_ability")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );
}

#[tokio::test]
async fn applies_version_range_and_transport_capability_defaults() {
    let mut env = HashMap::new();
    env.insert("COOLIFY_URL".into(), "https://coolify.example".into());
    env.insert("COOLIFY_TOKEN".into(), "token".into());
    env.insert("MCP_TRANSPORT".into(), "stdio".into());
    let report = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            if path == "/version" {
                Ok(response(200, "text/plain", "4.9.0"))
            } else {
                Ok(response(404, "application/json", "{}"))
            }
        }
    })
    .await;
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "version")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "deploy_ability")
            .unwrap()
            .status,
        DoctorCheckStatus::Pass
    );

    env.insert("MCP_TRANSPORT".into(), "http".into());
    let report = run_doctor(&env, |_path| async {
        Ok::<_, String>(response(200, "text/plain", "4.2.0"))
    })
    .await;
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "capability_profile")
            .unwrap()
            .detail,
        "HTTP defaults to read-only capability"
    );
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "deploy_ability")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );
}

#[test]
fn human_report_has_fix_guidance() {
    let report = DoctorReport {
        ok: false,
        checks: vec![],
    };
    assert!(report.to_human().contains("doctor"));
}
