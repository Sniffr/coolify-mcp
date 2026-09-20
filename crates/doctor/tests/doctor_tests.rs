use doctor::{DoctorCheckStatus, DoctorReport, ProbeResponse, run_doctor, run_hosted_doctor};
use std::collections::HashMap;

fn response(status: u16, content_type: &str, body: &str) -> ProbeResponse {
    ProbeResponse {
        status,
        content_type: Some(content_type.into()),
        body: body.into(),
        redirected: false,
        location: None,
    }
}

#[test]
fn hosted_doctor_does_not_require_global_coolify_credentials() {
    let env = HashMap::from([
        ("MCP_PUBLIC_URL".into(), "https://mcp.example".into()),
        ("MCP_CONNECTION_ENCRYPTION_KEY".into(), "test-key".into()),
        ("GITHUB_CLIENT_ID".into(), "client-id".into()),
        ("GITHUB_CLIENT_SECRET".into(), "client-secret".into()),
        (
            "GITHUB_CALLBACK_URL".into(),
            "https://mcp.example/auth/github/callback".into(),
        ),
    ]);
    let report = run_hosted_doctor(&env, true);
    assert!(report.ok);
    assert!(
        !report
            .checks
            .iter()
            .any(|check| check.name == "coolify_url")
    );
    assert!(!report.to_json().unwrap().contains("client-secret"));
}

fn redirect(status: u16, location: &str, body: &str) -> ProbeResponse {
    ProbeResponse {
        status,
        content_type: Some("text/html".into()),
        body: body.into(),
        redirected: true,
        location: Some(location.into()),
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

    let redirect_response = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            if path == "/__coolify_mcp_doctor_invalid__" {
                Ok(redirect(302, "/login", "location: /login"))
            } else {
                Ok(response(200, "text/plain", "4.2.1"))
            }
        }
    })
    .await;
    assert_eq!(
        redirect_response
            .checks
            .iter()
            .find(|c| c.name == "routing_catch_all")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );
}

#[tokio::test]
async fn html_without_html_tag_and_location_header_are_proxy_failures() {
    let mut env = HashMap::new();
    env.insert("COOLIFY_BASE_URL".into(), "https://coolify.example".into());
    env.insert("COOLIFY_ACCESS_TOKEN".into(), "redacted-token".into());
    // Challenge/login page with no literal `<html` marker must still fail routing.
    let head_only = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            if path == "/__coolify_mcp_doctor_invalid__" {
                Ok(response(
                    200,
                    "text/html",
                    "<head><title>Sign in</title></head><body>Just a moment, verifying your browser</body>",
                ))
            } else {
                Ok(response(200, "text/plain", "4.2.1"))
            }
        }
    })
    .await;
    assert_eq!(
        head_only
            .checks
            .iter()
            .find(|c| c.name == "routing_catch_all")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );
    // A JSON 404 that also carries a redirect Location is a proxy redirect, not valid routing.
    let located = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            if path == "/__coolify_mcp_doctor_invalid__" {
                Ok(ProbeResponse {
                    status: 404,
                    content_type: Some("application/json".into()),
                    body: "{\"message\":\"not found\"}".into(),
                    redirected: false,
                    location: Some("https://coolify.example/login".into()),
                })
            } else {
                Ok(response(200, "text/plain", "4.2.1"))
            }
        }
    })
    .await;
    assert_eq!(
        located
            .checks
            .iter()
            .find(|c| c.name == "routing_catch_all")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );
    // An HTML body on the version endpoint means the API path is not reaching Coolify.
    let version_html = run_doctor(&env, |path| {
        let path = path.to_owned();
        async move {
            if path == "/version" {
                Ok(response(
                    200,
                    "text/html",
                    "<head><title>Proxy Login</title></head>",
                ))
            } else {
                Ok(response(404, "application/json", "{}"))
            }
        }
    })
    .await;
    assert_eq!(
        version_html
            .checks
            .iter()
            .find(|c| c.name == "coolify_reachable")
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
                "/version" => Ok(response(
                    401,
                    "application/json",
                    "{\"message\":\"Unauthorized\"}",
                )),
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

#[tokio::test]
async fn deploy_ability_matches_effective_runtime_config() {
    async fn deploy_status(env: &HashMap<String, String>) -> doctor::DoctorCheckStatus {
        run_doctor(env, |path| {
            let path = path.to_owned();
            async move {
                if path == "/version" {
                    Ok(response(200, "text/plain", "4.2.0"))
                } else {
                    Ok(response(
                        404,
                        "application/json",
                        "{\"message\":\"not found\"}",
                    ))
                }
            }
        })
        .await
        .checks
        .into_iter()
        .find(|c| c.name == "deploy_ability")
        .unwrap()
        .status
    }

    // HTTP default is read-only: deploy must fail.
    let mut env = HashMap::new();
    env.insert("COOLIFY_URL".into(), "https://coolify.example".into());
    env.insert("COOLIFY_TOKEN".into(), "token".into());
    env.insert("MCP_TRANSPORT".into(), "http".into());
    assert_eq!(deploy_status(&env).await, DoctorCheckStatus::Fail);

    // Explicit operations profile over HTTP may pass when Coolify is reachable.
    env.insert("MCP_CAPABILITY_PROFILE".into(), "operations".into());
    assert_eq!(deploy_status(&env).await, DoctorCheckStatus::Pass);

    // stdio default is operations-compatible.
    env.insert("MCP_TRANSPORT".into(), "stdio".into());
    env.remove("MCP_CAPABILITY_PROFILE");
    assert_eq!(deploy_status(&env).await, DoctorCheckStatus::Pass);

    // MCP_READONLY wins over an explicit admin profile, exactly like the runtime.
    env.insert("MCP_READONLY".into(), "true".into());
    env.insert("MCP_CAPABILITY_PROFILE".into(), "admin".into());
    assert_eq!(deploy_status(&env).await, DoctorCheckStatus::Fail);

    // The removed MCP_DEPLOY_PERMISSION escape hatch must no longer grant ability.
    env.remove("MCP_READONLY");
    env.remove("MCP_CAPABILITY_PROFILE");
    env.insert("MCP_TRANSPORT".into(), "http".into());
    env.insert("MCP_DEPLOY_PERMISSION".into(), "true".into());
    assert_eq!(deploy_status(&env).await, DoctorCheckStatus::Fail);

    // Operations profile without reachable Coolify cannot claim deploy ability.
    let mut offline = HashMap::new();
    offline.insert("COOLIFY_URL".into(), "https://coolify.example".into());
    offline.insert("COOLIFY_TOKEN".into(), "token".into());
    offline.insert("MCP_CAPABILITY_PROFILE".into(), "operations".into());
    let report = run_doctor(&offline, |_path| async {
        Err::<ProbeResponse, _>("connection refused".into())
    })
    .await;
    assert_eq!(
        report
            .checks
            .into_iter()
            .find(|c| c.name == "deploy_ability")
            .unwrap()
            .status,
        DoctorCheckStatus::Inconclusive
    );
}

#[tokio::test]
async fn capability_profile_parsing_matches_runtime_case_insensitively() {
    async fn report_for(env: &HashMap<String, String>) -> Vec<doctor::DoctorCheck> {
        run_doctor(env, |path| {
            let path = path.to_owned();
            async move {
                if path == "/version" {
                    Ok(response(200, "text/plain", "4.2.0"))
                } else {
                    Ok(response(
                        404,
                        "application/json",
                        "{\"message\":\"not found\"}",
                    ))
                }
            }
        })
        .await
        .checks
    }
    fn status_of(checks: &[doctor::DoctorCheck], name: &str) -> DoctorCheckStatus {
        checks
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .status
            .clone()
    }

    // Uppercase/mixed-case values must be accepted exactly like the runtime,
    // which compares MCP_CAPABILITY_PROFILE case-insensitively.
    for (value, deploy) in [
        ("OPERATIONS", DoctorCheckStatus::Pass),
        ("Operations", DoctorCheckStatus::Pass),
        ("ADMIN", DoctorCheckStatus::Pass),
        ("Admin", DoctorCheckStatus::Pass),
        ("READ-ONLY", DoctorCheckStatus::Fail),
        ("Read-Only", DoctorCheckStatus::Fail),
    ] {
        let mut env = HashMap::new();
        env.insert("COOLIFY_URL".into(), "https://coolify.example".into());
        env.insert("COOLIFY_TOKEN".into(), "token".into());
        env.insert("MCP_TRANSPORT".into(), "http".into());
        env.insert("MCP_CAPABILITY_PROFILE".into(), value.into());
        let checks = report_for(&env).await;
        assert_eq!(
            status_of(&checks, "capability_profile"),
            DoctorCheckStatus::Pass,
            "profile {value} must be recognized"
        );
        assert_eq!(
            status_of(&checks, "deploy_ability"),
            deploy,
            "profile {value} deploy ability must match runtime"
        );
    }

    // A truly unknown value must still fail.
    let mut env = HashMap::new();
    env.insert("COOLIFY_URL".into(), "https://coolify.example".into());
    env.insert("COOLIFY_TOKEN".into(), "token".into());
    env.insert("MCP_CAPABILITY_PROFILE".into(), "superuser".into());
    let checks = report_for(&env).await;
    assert_eq!(
        status_of(&checks, "capability_profile"),
        DoctorCheckStatus::Fail
    );
    assert_eq!(
        status_of(&checks, "deploy_ability"),
        DoctorCheckStatus::Fail
    );
}

/// End-to-end through real HTTP: the fetcher below performs actual requests
/// with redirects disabled, exactly like the production doctor adapter, so
/// status, Content-Type, Location, and body shape must survive to classification.
#[tokio::test]
async fn local_proxy_shapes_are_rejected_end_to_end() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            handle_local_probe_connection(stream);
        }
    });
    let base = format!("http://{addr}");
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let mut env = HashMap::new();
    env.insert("COOLIFY_BASE_URL".into(), "http://127.0.0.1".into());
    env.insert("COOLIFY_ACCESS_TOKEN".into(), "redacted-token".into());
    let report = run_doctor(&env, |path| {
        let client = client.clone();
        let url = format!("{base}{path}");
        async move {
            let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
            let status = response.status().as_u16();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let redirected = (300..400).contains(&status) || location.is_some();
            let bytes = response.bytes().await.map_err(|e| e.to_string())?;
            let body = String::from_utf8_lossy(&bytes[..bytes.len().min(10_000)]).into_owned();
            Ok::<_, String>(ProbeResponse {
                status,
                content_type,
                body,
                redirected,
                location,
            })
        }
    })
    .await;
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "coolify_reachable")
            .unwrap()
            .status,
        DoctorCheckStatus::Pass
    );
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.name == "routing_catch_all")
            .unwrap()
            .status,
        DoctorCheckStatus::Fail
    );
    let json = report.to_json().unwrap();
    assert!(!json.contains("redacted-token"));
}

fn handle_local_probe_connection(mut stream: std::net::TcpStream) {
    use std::io::{Read, Write};
    let mut buf = vec![0u8; 4096];
    let Ok(n) = stream.read(&mut buf) else {
        return;
    };
    let request = String::from_utf8_lossy(&buf[..n]).into_owned();
    let target = request
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("/");
    let (status, headers, body) = if target == "/version" {
        ("200 OK", "content-type: text/plain", "4.2.1")
    } else {
        // Proxy-style redirect with a challenge body containing no `<html` marker.
        (
            "302 Found",
            "content-type: text/html\r\nlocation: /login",
            "Just a moment, verifying your browser",
        )
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\n{headers}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
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
