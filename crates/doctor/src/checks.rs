use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorCheckStatus {
    Pass,
    Fail,
    Inconclusive,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorCheckStatus,
    pub detail: String,
    pub fix: String,
}
impl DoctorCheck {
    pub fn pass(name: &str, detail: &str, fix: &str) -> Self {
        Self {
            name: name.into(),
            status: DoctorCheckStatus::Pass,
            detail: detail.into(),
            fix: fix.into(),
        }
    }
    pub fn fail(name: &str, detail: &str, fix: &str) -> Self {
        Self {
            name: name.into(),
            status: DoctorCheckStatus::Fail,
            detail: detail.into(),
            fix: fix.into(),
        }
    }
    pub fn inconclusive(name: &str, detail: &str, fix: &str) -> Self {
        Self {
            name: name.into(),
            status: DoctorCheckStatus::Inconclusive,
            detail: detail.into(),
            fix: fix.into(),
        }
    }
}

/// Sanitized metadata from one side-effect-free GET probe. Body is bounded by the caller.
#[derive(Clone, Debug)]
pub struct ProbeResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: String,
    pub redirected: bool,
}

pub(crate) fn static_checks(env: &HashMap<String, String>) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let url = env
        .get("COOLIFY_BASE_URL")
        .filter(|v| !v.trim().is_empty())
        .or_else(|| env.get("COOLIFY_URL"))
        .map(String::as_str);
    let token = env
        .get("COOLIFY_ACCESS_TOKEN")
        .filter(|v| !v.trim().is_empty())
        .or_else(|| env.get("COOLIFY_TOKEN"))
        .map(String::as_str);
    match url {
        None => checks.push(DoctorCheck::fail(
            "coolify_url",
            "Coolify URL is not configured",
            "Set COOLIFY_BASE_URL (or COOLIFY_URL) to the Coolify origin.",
        )),
        Some(raw) if Url::parse(raw).is_err() => checks.push(DoctorCheck::fail(
            "coolify_url",
            "Coolify URL is malformed",
            "Set a complete http(s) URL without shell placeholders.",
        )),
        Some(raw)
            if !matches!(
                Url::parse(raw)
                    .ok()
                    .map(|u| u.scheme().to_owned())
                    .as_deref(),
                Some("http") | Some("https")
            ) =>
        {
            checks.push(DoctorCheck::fail(
                "coolify_url",
                "Coolify URL must use HTTP or HTTPS",
                "Use an http:// or https:// Coolify URL.",
            ))
        }
        Some(_) => checks.push(DoctorCheck::pass(
            "coolify_url",
            "Coolify URL is configured",
            "No action required.",
        )),
    }
    if env
        .get("COOLIFY_ACCESS_TOKEN_FILE")
        .is_some_and(|v| !v.trim().is_empty())
    {
        checks.push(DoctorCheck::pass(
            "coolify_token",
            "Coolify access token file is configured",
            "No action required.",
        ));
    } else if token.is_none() {
        checks.push(DoctorCheck::fail(
            "coolify_token",
            "Coolify access token is not configured",
            "Set COOLIFY_ACCESS_TOKEN (or COOLIFY_TOKEN), or provide a token file.",
        ));
    } else {
        checks.push(DoctorCheck::pass(
            "coolify_token",
            "Coolify access token is configured",
            "No action required.",
        ));
    }
    let literal = env.values().any(|v| v.contains("${") || v.contains("$({"));
    checks.push(if literal {
        DoctorCheck::fail(
            "literal_variables",
            "Configuration contains an unresolved variable placeholder",
            "Expand environment variables before starting the server; never commit a real token.",
        )
    } else {
        DoctorCheck::pass(
            "literal_variables",
            "No unresolved variable placeholders found",
            "No action required.",
        )
    });
    let doubled = url.is_some_and(|v| v.contains("/api/v1"));
    checks.push(if doubled {
        DoctorCheck::fail(
            "api_path",
            "Coolify URL already contains /api/v1",
            "Set the Coolify origin only; the client adds /api/v1 itself.",
        )
    } else {
        DoctorCheck::pass(
            "api_path",
            "Coolify API path will be normalized once",
            "No action required.",
        )
    });
    if env
        .get("MCP_TRANSPORT")
        .is_some_and(|v| v.eq_ignore_ascii_case("http"))
    {
        match env.get("MCP_PUBLIC_URL") {
            Some(v) if v.starts_with("https://") => checks.push(DoctorCheck::pass(
                "public_url",
                "Hosted public URL uses HTTPS",
                "No action required.",
            )),
            Some(_) => checks.push(DoctorCheck::fail(
                "public_url",
                "Hosted public URL is not HTTPS",
                "Use HTTPS outside explicit local development.",
            )),
            None => checks.push(DoctorCheck::fail(
                "public_url",
                "Hosted public URL is missing",
                "Set MCP_PUBLIC_URL for HTTP mode.",
            )),
        }
    }
    let explicit = env.get("MCP_CAPABILITY_PROFILE").map(String::as_str);
    let profile = explicit.unwrap_or_else(|| {
        if env
            .get("MCP_TRANSPORT")
            .is_some_and(|v| v.eq_ignore_ascii_case("http"))
        {
            "read-only"
        } else {
            "operations"
        }
    });
    checks.push(if matches!(profile, "read-only" | "operations" | "admin") {
        DoctorCheck::pass(
            "capability_profile",
            if explicit.is_some() {
                "Capability profile is recognized"
            } else if profile == "read-only" {
                "HTTP defaults to read-only capability"
            } else {
                "stdio defaults to operations capability"
            },
            "No action required.",
        )
    } else {
        DoctorCheck::fail(
            "capability_profile",
            "Capability profile is invalid",
            "Use read-only, operations, or admin.",
        )
    });
    checks
}

pub(crate) async fn network_checks<F, Fut>(
    env: &HashMap<String, String>,
    fetcher: &F,
) -> Vec<DoctorCheck>
where
    F: Fn(&str) -> Fut,
    Fut: Future<Output = Result<ProbeResponse, String>>,
{
    let mut checks = Vec::new();
    match bounded(fetcher("/version")).await {
        Ok(response) if (200..300).contains(&response.status) => {
            let parts = response.body.trim().split('.').take(2).collect::<Vec<_>>();
            let valid = parts.len() == 2
                && parts[0] == "4"
                && parts[1].parse::<u64>().is_ok_and(|minor| minor <= 3);
            checks.push(if valid {
                DoctorCheck::pass(
                    "version",
                    "Coolify reports a supported v4.0-v4.3 version",
                    "No action required.",
                )
            } else {
                DoctorCheck::fail(
                    "version",
                    "Coolify version is outside the supported v4.0-v4.3 range",
                    "Upgrade or use a compatible Coolify release.",
                )
            });
            checks.push(DoctorCheck::pass(
                "coolify_reachable",
                "Coolify responded to a read-only version probe",
                "No action required.",
            ));
        }
        Ok(response) if response.status == 401 || response.status == 403 => {
            checks.push(DoctorCheck::fail(
                "token_valid",
                "Coolify rejected the access token",
                "Create a valid Coolify API token and inject it at runtime.",
            ));
            checks.push(DoctorCheck::inconclusive(
                "coolify_reachable",
                "Coolify is reachable but authorization failed",
                "Fix the token, then run doctor again.",
            ));
            checks.push(DoctorCheck::inconclusive(
                "version",
                "Coolify version could not be verified",
                "Restore authorization, then run doctor again.",
            ));
        }
        Ok(response)
            if response.redirected
                || (300..400).contains(&response.status)
                || is_html(&response) =>
        {
            checks.push(DoctorCheck::fail(
                "coolify_reachable",
                "Coolify probe was redirected by a proxy or Cloudflare",
                "Check DNS, proxy, TLS, and the Coolify URL.",
            ));
            checks.push(DoctorCheck::inconclusive(
                "version",
                "Coolify version could not be verified",
                "Restore direct API access, then run doctor again.",
            ));
        }
        Ok(_) => {
            checks.push(DoctorCheck::fail(
                "coolify_reachable",
                "Coolify did not answer the read-only probe",
                "Check DNS, proxy, firewall, and the Coolify URL.",
            ));
            checks.push(DoctorCheck::inconclusive(
                "version",
                "Coolify version could not be verified",
                "Restore connectivity, then run doctor again.",
            ));
        }
        Err(error)
            if error.to_ascii_lowercase().contains("401")
                || error.to_ascii_lowercase().contains("unauthorized") =>
        {
            checks.push(DoctorCheck::fail(
                "token_valid",
                "Coolify rejected the access token",
                "Create a valid Coolify API token and inject it at runtime.",
            ));
            checks.push(DoctorCheck::inconclusive(
                "coolify_reachable",
                "Coolify is reachable but authorization failed",
                "Fix the token, then run doctor again.",
            ));
            checks.push(DoctorCheck::inconclusive(
                "version",
                "Coolify version could not be verified",
                "Restore authorization, then run doctor again.",
            ));
        }
        Err(_) => {
            checks.push(DoctorCheck::inconclusive(
                "coolify_reachable",
                "Coolify did not answer the read-only probe",
                "Check DNS, proxy, firewall, and the Coolify URL.",
            ));
            checks.push(DoctorCheck::inconclusive(
                "version",
                "Coolify version could not be verified",
                "Restore connectivity, then run doctor again.",
            ));
        }
    }
    // Deliberately invalid API route: a valid API error proves routing without touching /mcp or mutating state.
    match bounded(fetcher("/__coolify_mcp_doctor_invalid__")).await {
        Ok(response)
            if (400..500).contains(&response.status)
                && is_json(&response)
                && !response.redirected =>
        {
            checks.push(DoctorCheck::pass(
                "routing_catch_all",
                "Invalid API route returned a structured JSON error",
                "No action required.",
            ))
        }
        Ok(response)
            if response.redirected
                || (300..400).contains(&response.status)
                || is_html(&response)
                || (200..300).contains(&response.status) =>
        {
            checks.push(DoctorCheck::fail(
                "routing_catch_all",
                "Invalid API route was handled by a proxy catch-all or redirect",
                "Ensure the reverse proxy forwards the Coolify API path without an HTML rewrite.",
            ))
        }
        Ok(_) | Err(_) => checks.push(DoctorCheck::inconclusive(
            "routing_catch_all",
            "Invalid API route probe could not be classified",
            "Check the Coolify API route and proxy response headers.",
        )),
    }
    let explicit_permission = env.get("MCP_DEPLOY_PERMISSION").map(|v| {
        matches!(
            v.to_ascii_lowercase().as_str(),
            "true" | "yes" | "operations" | "admin"
        )
    });
    let profile = env
        .get("MCP_CAPABILITY_PROFILE")
        .map(String::as_str)
        .unwrap_or_else(|| {
            if env
                .get("MCP_TRANSPORT")
                .is_some_and(|v| v.eq_ignore_ascii_case("http"))
            {
                "read-only"
            } else {
                "operations"
            }
        });
    let deploy_allowed = explicit_permission.unwrap_or(matches!(profile, "operations" | "admin"));
    checks.push(if deploy_allowed { DoctorCheck::pass("deploy_ability", "Configured capability profile permits deployment operations", "No action required.") } else { DoctorCheck::fail("deploy_ability", "No deployment capability is configured", "Use an operations/admin profile or explicitly configure deploy permission; doctor never triggers deployment.") });
    checks
}

fn is_json(response: &ProbeResponse) -> bool {
    response
        .content_type
        .as_deref()
        .is_some_and(|v| v.to_ascii_lowercase().contains("json"))
}
fn is_html(response: &ProbeResponse) -> bool {
    response
        .content_type
        .as_deref()
        .is_some_and(|v| v.to_ascii_lowercase().contains("html"))
        || response.body.to_ascii_lowercase().contains("<html")
        || response.body.to_ascii_lowercase().contains("cloudflare")
}
async fn bounded<Fut>(future: Fut) -> Result<ProbeResponse, String>
where
    Fut: Future<Output = Result<ProbeResponse, String>>,
{
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .map_err(|_| "probe timed out after ten seconds".into())
        .and_then(|v| v)
}
