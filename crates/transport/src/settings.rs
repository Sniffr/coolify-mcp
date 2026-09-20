use safety::CapabilityProfile;
use url::Url;

pub(crate) fn parse_profile(value: &str) -> Option<CapabilityProfile> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read-only" | "readonly" | "read_only" => Some(CapabilityProfile::ReadOnly),
        "operations" | "operation" => Some(CapabilityProfile::Operations),
        "admin" => Some(CapabilityProfile::Admin),
        _ => None,
    }
}

pub(crate) fn profile_name(profile: CapabilityProfile) -> &'static str {
    match profile {
        CapabilityProfile::ReadOnly => "read-only",
        CapabilityProfile::Operations => "operations",
        CapabilityProfile::Admin => "admin",
    }
}

pub(crate) fn validate_base_url(raw: &str, allow_insecure_local_targets: bool) -> Result<Url, ()> {
    let mut url = Url::parse(raw.trim()).map_err(|_| ())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(());
    }
    while url.path().ends_with('/') && url.path() != "/" {
        let path = url.path().trim_end_matches('/').to_owned();
        url.set_path(&path);
    }
    // This escape hatch exists only for debug/test local acceptance. Release
    // hosted builds never enable it; production must validate public HTTPS.
    if !allow_insecure_local_targets && coolify_api::validate_hosted_base_url(&url).is_err() {
        return Err(());
    }
    Ok(url)
}

pub(crate) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

pub(crate) fn settings_html(
    host: Option<&str>,
    profile: Option<CapabilityProfile>,
    csrf: &str,
) -> String {
    let host = host.map(html_escape).unwrap_or_default();
    let profile = profile.map(profile_name).unwrap_or("read-only");
    let configured = !host.is_empty();
    let status_banner = if configured {
        format!(
            "<p><strong>Connected to {host} ({profile}).</strong> Claude can now use your saved Coolify connection.</p>"
        )
    } else {
        "<p><strong>No Coolify connection yet.</strong> Paste your Base URL and API token below, press Save, then return to Claude Code/Desktop and retry your request.</p><ol><li>Use the same browser session you used for GitHub login.</li><li>Base URL must be public https with no <code>/api/v1</code> suffix (example: <code>https://coolify.example.com</code>).</li><li>After Save, retry in Claude — missing-connection tool errors link back here.</li></ol>".to_owned()
    };
    format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>Coolify settings</title><main><h1>Coolify settings</h1>{status_banner}<p>Token configured: {configured}</p><p>The token is encrypted per-user and is never displayed.</p><form method=post action='/settings/coolify'><input type=hidden name=csrf value='{csrf}'><label>Base URL <input name=base_url value='{host}' required></label><label>Access token <input name=access_token type=password required autocomplete=off></label><label>Profile <select name=profile><option {ro}>read-only</option><option {ops}>operations</option><option {admin}>admin</option></select></label><button>Save</button></form><form method=post action='/settings/logout'><input type=hidden name=csrf value='{csrf}'><button>Log out</button></form><form method=post action='/settings/coolify'><input type=hidden name=csrf value='{csrf}'><input type=hidden name=delete value=true><button>Delete connection</button></form></main>",
        ro = if profile == "read-only" {
            "selected"
        } else {
            ""
        },
        ops = if profile == "operations" {
            "selected"
        } else {
            ""
        },
        admin = if profile == "admin" { "selected" } else { "" },
    )
}
