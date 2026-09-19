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
    match token {
        None => checks.push(DoctorCheck::fail(
            "coolify_token",
            "Coolify access token is not configured",
            "Set COOLIFY_ACCESS_TOKEN (or COOLIFY_TOKEN), or provide a token file.",
        )),
        Some(_) => checks.push(DoctorCheck::pass(
            "coolify_token",
            "Coolify access token is configured",
            "No action required.",
        )),
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
    let profile = env
        .get("MCP_CAPABILITY_PROFILE")
        .map(String::as_str)
        .unwrap_or("operations");
    checks.push(if matches!(profile, "read-only" | "operations" | "admin") {
        DoctorCheck::pass(
            "capability_profile",
            "Capability profile is recognized",
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

pub(crate) async fn network_checks<F, Fut>(fetcher: &F) -> Vec<DoctorCheck>
where
    F: Fn(&str) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let mut checks = Vec::new();
    let version = bounded(fetcher("/version")).await;
    match version {
        Ok(value) => {
            let supported = value.trim().split('.').take(2).collect::<Vec<_>>();
            let valid = supported.len() == 2
                && supported[0] == "4"
                && supported[1]
                    .parse::<u64>()
                    .is_ok_and(|minor| (0..=3).contains(&minor));
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
        Err(error) => {
            let lower = error.to_ascii_lowercase();
            let detail = if lower.contains("401") || lower.contains("unauthorized") {
                "Coolify rejected the access token"
            } else if lower.contains("redirect") || lower.contains("cloudflare") {
                "Coolify probe was redirected by a proxy or Cloudflare"
            } else {
                "Coolify did not answer the read-only probe"
            };
            let name = if lower.contains("401") || lower.contains("unauthorized") {
                "token_valid"
            } else {
                "coolify_reachable"
            };
            checks.push(DoctorCheck::fail(
                name,
                detail,
                if name == "token_valid" {
                    "Create a valid Coolify API token and inject it at runtime."
                } else {
                    "Check DNS, proxy, firewall, and the Coolify URL."
                },
            ));
            checks.push(DoctorCheck::inconclusive(
                "version",
                "Coolify version could not be verified",
                "Restore connectivity, then run doctor again.",
            ));
        }
    }
    for (name, path, fix) in [
        (
            "routing_catch_all",
            "/mcp",
            "Ensure the reverse proxy forwards /mcp without a catch-all HTML rewrite.",
        ),
        (
            "deploy_ability",
            "/applications",
            "Use an operations/admin profile and a token permitted to read application state.",
        ),
    ] {
        match bounded(fetcher(path)).await {
            Ok(_) => checks.push(DoctorCheck::pass(
                name,
                "Read-only endpoint probe succeeded",
                "No action required.",
            )),
            Err(error) if error.contains("401") || error.contains("403") => checks.push(
                DoctorCheck::fail(name, "Coolify denied the read-only capability probe", fix),
            ),
            Err(error) if error.to_ascii_lowercase().contains("html") => {
                checks.push(DoctorCheck::fail(
                    name,
                    "Proxy returned an HTML catch-all instead of the expected endpoint",
                    fix,
                ))
            }
            Err(_) => checks.push(DoctorCheck::inconclusive(
                name,
                "Endpoint probe could not be completed",
                fix,
            )),
        }
    }
    checks
}

async fn bounded<Fut>(future: Fut) -> Result<String, String>
where
    Fut: Future<Output = Result<String, String>>,
{
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .map_err(|_| "probe timed out after ten seconds".into())
        .and_then(|v| v)
}
