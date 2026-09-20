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

pub(crate) fn validate_base_url(raw: &str) -> Result<Url, ()> {
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
    format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>Coolify settings</title><main><h1>Coolify settings</h1><p>Token configured: {}</p><p>The token is encrypted and is never displayed.</p><form method=post action='/settings/coolify'><input type=hidden name=csrf value='{}'><label>Base URL <input name=base_url value='{}' required></label><label>Access token <input name=access_token type=password required autocomplete=off></label><label>Profile <select name=profile><option {}>read-only</option><option {}>operations</option><option {}>admin</option></select></label><button>Save</button></form><form method=post action='/settings/logout'><input type=hidden name=csrf value='{}'><button>Log out</button></form><form method=post action='/settings/coolify'><input type=hidden name=csrf value='{}'><input type=hidden name=delete value=true><button>Delete connection</button></form></main>",
        !host.is_empty(),
        csrf,
        host,
        if profile == "read-only" {
            "selected"
        } else {
            ""
        },
        if profile == "operations" {
            "selected"
        } else {
            ""
        },
        if profile == "admin" { "selected" } else { "" },
        csrf,
        csrf
    )
}
