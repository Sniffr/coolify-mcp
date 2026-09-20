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
/// `location` carries the redirect Location header when present so proxy redirects
/// remain detectable after automatic redirect handling is disabled.
#[derive(Clone, Debug)]
pub struct ProbeResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: String,
    pub redirected: bool,
    pub location: Option<String>,
}

/// Effective capability exactly as the server runtime resolves it: `MCP_READONLY=true`
/// forces read-only, then an explicit `MCP_CAPABILITY_PROFILE`, then the transport
/// default (operations for stdio, read-only otherwise).
pub(crate) fn effective_profile(env: &HashMap<String, String>) -> &'static str {
    if env
        .get("MCP_READONLY")
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    {
        return "read-only";
    }
    if let Some(profile) = env.get("MCP_CAPABILITY_PROFILE") {
        if profile.eq_ignore_ascii_case("read-only") {
            return "read-only";
        }
        if profile.eq_ignore_ascii_case("operations") {
            return "operations";
        }
        if profile.eq_ignore_ascii_case("admin") {
            return "admin";
        }
        return "invalid";
    }
    let transport = env
        .get("MCP_TRANSPORT")
        .map(String::as_str)
        .unwrap_or("stdio");
    if transport.eq_ignore_ascii_case("stdio") {
        "operations"
    } else {
        "read-only"
    }
}

pub(crate) fn hosted_checks(env: &HashMap<String, String>, tenant_ready: bool) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let public_url = env.get("MCP_PUBLIC_URL").map(String::as_str);
    checks.push(match public_url.and_then(|value| Url::parse(value).ok()) {
        Some(url) if url.scheme() == "https" && url.host_str().is_some() => DoctorCheck::pass(
            "hosted_public_url",
            "Hosted public URL is configured",
            "No action required.",
        ),
        _ => DoctorCheck::fail(
            "hosted_public_url",
            "Hosted public URL is missing or invalid",
            "Set MCP_PUBLIC_URL to the hosted service HTTPS URL.",
        ),
    });
    for (name, variable, fix) in [
        (
            "hosted_encryption_key",
            "MCP_CONNECTION_ENCRYPTION_KEY",
            "Set MCP_CONNECTION_ENCRYPTION_KEY for tenant connection encryption.",
        ),
        (
            "hosted_github_client",
            "GITHUB_CLIENT_ID",
            "Set the hosted GitHub OAuth client ID.",
        ),
        (
            "hosted_github_secret",
            "GITHUB_CLIENT_SECRET",
            "Set the hosted GitHub OAuth client secret.",
        ),
        (
            "hosted_github_callback",
            "GITHUB_CALLBACK_URL",
            "Set GITHUB_CALLBACK_URL to the hosted GitHub callback URL.",
        ),
    ] {
        checks.push(
            if env
                .get(variable)
                .is_some_and(|value| !value.trim().is_empty())
            {
                DoctorCheck::pass(
                    name,
                    "Hosted identity or tenant setting is configured",
                    "No action required.",
                )
            } else {
                DoctorCheck::fail(name, "Hosted identity or tenant setting is missing", fix)
            },
        );
    }
    checks.push(if tenant_ready {
        DoctorCheck::pass(
            "tenant_persistence",
            "Tenant persistence is available",
            "No action required.",
        )
    } else {
        DoctorCheck::fail(
            "tenant_persistence",
            "Tenant persistence is unavailable",
            "Check MCP_DATABASE_PATH and MCP_CONNECTION_ENCRYPTION_KEY.",
        )
    });
    checks
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
    let explicit = env.get("MCP_CAPABILITY_PROFILE").is_some();
    let readonly_enforced = env
        .get("MCP_READONLY")
        .is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let profile = effective_profile(env);
    checks.push(if matches!(profile, "read-only" | "operations" | "admin") {
        DoctorCheck::pass(
            "capability_profile",
            if readonly_enforced {
                "Read-only capability enforced by MCP_READONLY"
            } else if explicit {
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
    let mut reachable_ok = false;
    match bounded(fetcher("/version")).await {
        Ok(response) if (200..300).contains(&response.status) && !is_html(&response) => {
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
            reachable_ok = true;
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
        Ok(response) if is_redirect(&response) || is_html(&response) => {
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
                && !is_redirect(&response) =>
        {
            checks.push(DoctorCheck::pass(
                "routing_catch_all",
                "Invalid API route returned a structured JSON error",
                "No action required.",
            ))
        }
        Ok(response)
            if is_redirect(&response)
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
    // Deploy ability is inferred from the effective runtime capability only; doctor
    // never triggers a deployment. A pass additionally requires a reachable Coolify
    // so an offline host cannot claim deploy ability.
    let profile = effective_profile(env);
    let deploy_allowed = matches!(profile, "operations" | "admin");
    checks.push(if deploy_allowed && reachable_ok {
        DoctorCheck::pass(
            "deploy_ability",
            "Configured capability profile permits deployment operations",
            "No action required.",
        )
    } else if deploy_allowed {
        DoctorCheck::inconclusive(
            "deploy_ability",
            "Deploy capability is configured but Coolify was not reachable",
            "Restore connectivity, then run doctor again; doctor never triggers deployment.",
        )
    } else {
        DoctorCheck::fail(
            "deploy_ability",
            "No deployment capability is configured",
            "Use an operations/admin profile without MCP_READONLY; doctor never triggers deployment.",
        )
    });
    checks
}

fn is_json(response: &ProbeResponse) -> bool {
    response
        .content_type
        .as_deref()
        .is_some_and(|v| v.to_ascii_lowercase().contains("json"))
}
fn is_redirect(response: &ProbeResponse) -> bool {
    response.redirected
        || (300..400).contains(&response.status)
        || response.location.as_deref().is_some_and(|v| !v.is_empty())
}
fn is_html(response: &ProbeResponse) -> bool {
    if response
        .content_type
        .as_deref()
        .is_some_and(|v| v.to_ascii_lowercase().contains("html"))
    {
        return true;
    }
    let lower = response.body.to_ascii_lowercase();
    [
        "<html",
        "<!doctype",
        "<head",
        "<body",
        "<title",
        "cloudflare",
        "just a moment",
        "attention required",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
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
