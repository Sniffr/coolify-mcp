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
    let csrf_esc = html_escape(csrf);

    let (pill_class, pill_dot, pill_text) = if configured {
        ("pill ok", "<span class=dot></span>", "Connected")
    } else {
        ("pill warn", "<span class=dot></span>", "Setup needed")
    };
    let status_title = if configured {
        format!("Connected to {host}")
    } else {
        "Connect your Coolify".to_owned()
    };
    let status_sub = if configured {
        format!(
            "Profile <strong>{profile}</strong>. Claude can now use your saved connection. Your token stays encrypted per user and is never shown.",
        )
    } else {
        "Save your Coolify URL and API token below, then return to Claude Code or Claude Desktop and retry. It takes under a minute.".to_owned()
    };
    let steps = if configured {
        String::new()
    } else {
        r#"<ol class=steps><li><span>Use the same browser session you used for GitHub login, otherwise this page cannot see your session.</span></li><li><span>Paste a public <code>https://</code> URL with no <code>/api/v1</code> suffix.</span></li><li><span>Press <strong>Save connection</strong>, then retry in Claude.</span></li></ol>"#.to_owned()
    };
    let url_hint = if configured {
        String::new()
    } else {
        r#"<p class=hint id=urlHint aria-live=polite>Example: <code>https://coolify.example.com</code></p>"#.to_owned()
    };

    format!(
        r#"<!doctype html><html lang=en><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>Coolify connection</title><style>
:root{{color-scheme:light;--ink:#0f1e2e;--paper:#f3f6fa;--card:#ffffff;--line:#d9e1ec;--muted:#475467;--signal:#0e9384;--signal-deep:#0a6b5e;--ok:#027a48;--ok-bg:#ecfdf3;--warn:#b54708;--warn-bg:#fffaeb;--bad:#b42318;--focus:#175cd3;--radius:16px}}
*{{box-sizing:border-box}}body{{margin:0;background:var(--paper);color:var(--ink);font:16px/1.6 -apple-system,BlinkMacSystemFont,"SF Pro Text",Inter,"Segoe UI",Roboto,Helvetica,Arial,sans-serif}}
.signalbar{{height:5px;background:linear-gradient(90deg,var(--signal) 0%,#2dd4bf 55%,#0ea5e9 100%)}}
.wrap{{max-width:700px;margin:0 auto;padding:32px 20px 64px}}
.top{{display:flex;align-items:center;gap:10px;margin:6px 0 14px;color:var(--muted);font-size:14px}}
.top svg{{flex:none}}.pill{{display:inline-flex;align-items:center;gap:8px;border:1px solid var(--line);border-radius:999px;padding:5px 12px;font-size:13.5px;font-weight:650;background:#fff}}
.pill.ok{{border-color:#a6f4c5;background:var(--ok-bg);color:var(--ok)}}.pill.warn{{border-color:#fedf89;background:var(--warn-bg);color:var(--warn)}}
.dot{{width:9px;height:9px;border-radius:50%;background:currentColor}}@media (prefers-reduced-motion:no-preference){{.pill.warn .dot{{animation:pulse 1.8s ease-out 1}}.pill.ok .dot{{animation:none}}@keyframes pulse{{0%{{box-shadow:0 0 0 0 rgba(181,71,8,.45)}}100%{{box-shadow:0 0 0 10px rgba(181,71,8,0)}}}}}}
h1{{font-size:29px;line-height:1.2;letter-spacing:-.02em;margin:8px 0 6px}}.sub{{color:var(--muted);max-width:62ch;margin:0 0 16px}}
.panel{{background:var(--card);border:1px solid var(--line);border-radius:var(--radius);padding:22px;box-shadow:0 1px 2px rgba(16,24,40,.05)}}
.steps{{margin:14px 0 4px;padding:0;list-style:none;display:grid;gap:10px}}.steps li{{display:flex;gap:12px;align-items:flex-start;background:#f8fafc;border:1px solid var(--line);border-radius:12px;padding:11px 13px;counter-increment:step}}.steps ol,.steps{{counter-reset:step}}.steps li::before{{content:counter(step);counter-increment:step;flex:none;width:26px;height:26px;border-radius:50%;background:var(--ink);color:#fff;font-size:13.5px;font-weight:700;display:grid;place-items:center;margin-top:1px}}
code{{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.86em;background:#eef2f7;border:1px solid var(--line);border-radius:6px;padding:1px 6px}}
label.field{{display:block;margin:16px 0 4px;font-weight:650}}input[type=text],input[type=url],input[type=password],select{{width:100%;border:1px solid var(--line);border-radius:10px;padding:11px 12px;font:inherit;background:#fff;color:var(--ink)}}input:focus-visible,select:focus-visible,button:focus-visible{{outline:3px solid rgba(23,92,211,.35);outline-offset:1px;border-color:var(--focus)}}
.hint{{font-size:13.5px;color:var(--muted);margin:6px 0 0}}.hint.bad{{color:var(--bad);font-weight:600}}
fieldset{{border:1px solid var(--line);border-radius:12px;margin:18px 0 0;padding:14px}}legend{{font-weight:700;padding:0 8px}}.radio{{display:block;border:1px solid var(--line);border-radius:10px;padding:10px 12px;margin:8px 0;cursor:pointer}}.radio input{{margin-right:9px}}.radio small{{display:block;color:var(--muted);margin:2px 0 0 26px}}
.row{{display:flex;gap:12px;flex-wrap:wrap;margin-top:20px}}button{{font:inherit;font-weight:700;border-radius:10px;padding:11px 18px;cursor:pointer;border:1px solid transparent}}
.primary{{background:var(--signal);color:#fff}}.primary:hover{{background:var(--signal-deep)}}.ghost{{background:#fff;border-color:var(--line);color:var(--ink)}}
.danger-zone{{margin-top:22px;border:1px dashed #f1b0aa;border-radius:var(--radius);padding:18px 22px;background:#fff5f4}}.danger-zone h2{{font-size:16px;margin:0 0 4px}}.danger-zone p{{color:var(--muted);font-size:14px;margin:0 0 10px}}.danger{{background:#fff;border-color:#d92d20;color:var(--bad)}}
.meta{{margin-top:18px;font-size:13px;color:var(--muted)}}@media (max-width:520px){{.wrap{{padding:22px 14px 48px}}h1{{font-size:24px}}.panel{{padding:16px}}}}
</style><div class=signalbar></div><div class=wrap><div class=top><svg width=22 height=22 viewBox='0 0 24 24' fill=none aria-hidden=true><rect x=3 y=3 width=18 height=14 rx=2.5 stroke='#0f1e2e' stroke-width=1.8 /><path d='M8 21h8M12 17v4' stroke='#0e9384' stroke-width=1.8 stroke-linecap=round /><circle cx=7.5 cy=9.5 r=1.4 fill='#0e9384' /><path d='M11 9.5h6M11 12.5h6' stroke='#0f1e2e' stroke-width=1.6 stroke-linecap=round /></svg><span>Hosted Coolify connection</span></div>
<div class="{pill_class}">{pill_dot}{pill_text}</div><h1>{status_title}</h1><p class=sub>{status_sub}</p>{steps}<div class=panel>
<form method=post action='/settings/coolify' id=saveForm><input type=hidden name=csrf value='{csrf_esc}'>
<label class=field for=base_url>Coolify base URL</label><input id=base_url name=base_url type=url inputmode=url placeholder='https://coolify.example.com' value='{host}' required autocomplete=url>{url_hint}
<label class=field for=access_token>API token</label><input id=access_token name=access_token type=password required autocomplete=off placeholder='Paste token — it is encrypted and never shown again'>
<fieldset><legend>Capability</legend>
<label class=radio><input type=radio name=profile value=read-only{ro}><strong>Read-only</strong> — safest. List and inspect only.<small>Choose this unless you need restarts or deploys.</small></label>
<label class=radio><input type=radio name=profile value=operations{ops}><strong>Operations</strong> — restart, redeploy with confirmation.<small>Writes need explicit approval in Claude.</small></label>
<label class=radio><input type=radio name=profile value=admin{admin}><strong>Admin</strong> — full control including destructive actions.<small>Only for operators who need it.</small></label>
</fieldset><div class=row><button class=primary type=submit>Save connection</button></div></form></div>
<div class=danger-zone><h2>Session and data</h2><p>Logging out revokes this browser session. Deleting removes your saved URL and encrypted token.</p><div class=row><form method=post action='/settings/logout'><input type=hidden name=csrf value='{csrf_esc}'><button class=ghost type=submit>Log out</button></form><form method=post action='/settings/coolify'><input type=hidden name=csrf value='{csrf_esc}'><input type=hidden name=delete value=true><button class=danger type=submit>Delete connection</button></form></div></div>
<p class=meta>Token configured: <strong>{configured}</strong>. After saving, return to Claude and run <code>claude mcp list</code>. It should show connected.</p></div>
<script>(function(){{var u=document.getElementById('base_url'),h=document.getElementById('urlHint');if(!u||!h)return;function check(){{var v=(u.value||'').trim(),msg='Example: <code>https://coolify.example.com</code>',bad=false;if(/\/api\/v1/i.test(v)){{msg='Remove <code>/api/v1</code> — save only the base URL.';bad=true;}}else if(/^http:\/\//i.test(v)){{msg='Use <code>https://</code> for hosted Coolify.';bad=true;}}else if(/\/$/.test(v)&&v.length>8){{msg='Trailing <code>/</code> is fine — it will be trimmed.';}}h.innerHTML=msg;h.classList.toggle('bad',bad);}}u.addEventListener('input',check);check();}})();</script>"#,
        pill_class = pill_class,
        pill_dot = pill_dot,
        pill_text = pill_text,
        ro = if profile == "read-only" {
            " checked"
        } else {
            ""
        },
        ops = if profile == "operations" {
            " checked"
        } else {
            ""
        },
        admin = if profile == "admin" { " checked" } else { "" },
    )
}
