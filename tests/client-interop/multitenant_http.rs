//! Deterministic two-user hosted acceptance client. It never prints credentials or raw bodies.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, Response, StatusCode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use url::Url;

const ROSTER: [&str; 23] = [
    "application_logs",
    "diagnose_app",
    "diagnose_server",
    "find_issues",
    "get_application",
    "get_database",
    "get_infrastructure_overview",
    "get_mcp_version",
    "get_server",
    "get_service",
    "get_version",
    "list_applications",
    "list_databases",
    "list_deployments",
    "list_destinations",
    "list_servers",
    "list_services",
    "logs",
    "search_docs",
    "server_domains",
    "server_resources",
    "teams",
    "list_instances",
];
const REDIRECT: &str = "https://client.test/callback";
fn cookie(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers.get_all("set-cookie").iter().find_map(|v| {
        let s = v.to_str().ok()?;
        let first = s.split(';').next()?;
        first.strip_prefix(&format!("{name}=")).map(str::to_owned)
    })
}
async fn json(response: Response) -> Result<Value, Box<dyn std::error::Error>> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(format!("HTTP status {status}").into());
    }
    Ok(serde_json::from_str(&body)?)
}
async fn rpc(
    client: &Client,
    base: &str,
    bearer: &str,
    session: Option<&str>,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut request = client
        .post(format!("{base}/mcp"))
        .bearer_auth(bearer)
        .header("accept", "application/json")
        .json(&serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    let value = json(request.send().await?).await?;
    if value["error"].is_object() {
        return Err("JSON-RPC error".into());
    }
    Ok(value)
}
#[allow(clippy::too_many_arguments)]
async fn user_flow(
    client: &Client,
    base: &str,
    github_base: &str,
    coolify_base: &str,
    code: &str,
    token: &str,
    expected_app: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = json(
        client
            .post(format!("{base}/oauth/state"))
            .json(&serde_json::json!({"client_id":client_id,"redirect_uri":REDIRECT}))
            .send()
            .await?,
    )
    .await?["state"]
        .as_str()
        .ok_or("missing state")?
        .to_owned();
    let verifier = format!("fixture-verifier-{code}-012345678901234567890123456789");
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let auth = client
        .get(format!("{base}/oauth/authorize"))
        .query(&[
            ("client_id", client_id),
            ("redirect_uri", REDIRECT),
            ("response_type", "code"),
            ("resource", &format!("{base}/mcp")),
            ("scope", "mcp"),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ])
        .send()
        .await?;
    assert_eq!(auth.status(), StatusCode::SEE_OTHER);
    let start = auth
        .headers()
        .get("location")
        .ok_or("missing auth redirect")?
        .to_str()?
        .to_owned();
    let start_url = if start.starts_with("http") {
        start.clone()
    } else {
        format!("{base}{start}")
    };
    let start = client.get(start_url).send().await?;
    assert_eq!(start.status(), StatusCode::SEE_OTHER);
    let github_state =
        cookie(start.headers(), "mcp_github_state").ok_or("missing github state cookie")?;
    let github_location = start
        .headers()
        .get("location")
        .ok_or("missing github redirect")?
        .to_str()?;
    let github_query: HashMap<_, _> = Url::parse(github_location)?
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(github_query.get("state"), Some(&github_state));
    // The fixture authorization endpoint is represented by its deterministic code; the callback still performs the real provider exchange.
    let callback = client
        .get(format!(
            "{base}/auth/github/callback?code={code}&state={github_state}"
        ))
        .header("cookie", format!("mcp_github_state={github_state}"))
        .send()
        .await?;
    assert_eq!(callback.status(), StatusCode::SEE_OTHER);
    let session = cookie(callback.headers(), "mcp_session").ok_or("missing browser session")?;
    let csrf = cookie(callback.headers(), "mcp_csrf").ok_or("missing csrf")?;
    let redirect = Url::parse(
        callback
            .headers()
            .get("location")
            .ok_or("missing client redirect")?
            .to_str()?,
    )?;
    let mcp_code = redirect
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .ok_or("missing mcp code")?;
    let token_response = json(
        client
            .post(format!("{base}/oauth/token"))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &mcp_code),
                ("redirect_uri", REDIRECT),
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("code_verifier", &verifier),
                ("resource", &format!("{base}/mcp")),
            ])
            .send()
            .await?,
    )
    .await?;
    let bearer = token_response["access_token"]
        .as_str()
        .ok_or("missing bearer")?;
    let settings = json(
        client
            .get(format!("{base}/settings"))
            .header("accept", "application/json")
            .header("cookie", format!("mcp_session={session}"))
            .send()
            .await?,
    )
    .await?;
    assert_eq!(settings["configured"], false);
    let saved=json(client.post(format!("{base}/settings/coolify")).header("accept","application/json").header("cookie",format!("mcp_session={session}")).header("x-csrf-token",&csrf).json(&serde_json::json!({"base_url":coolify_base,"access_token":token,"profile":"read-only"})).send().await?).await?;
    assert_eq!(saved["configured"], true);
    assert!(!saved.to_string().contains(token));
    let initialized = rpc(
        client,
        base,
        bearer,
        None,
        1,
        "initialize",
        serde_json::json!({}),
    )
    .await?;
    let mcp_session = initialized["result"]
        .get("serverInfo")
        .ok_or("initialize failed")?;
    assert_eq!(mcp_session["name"], "coolify-mcp");
    let init_response = client
        .post(format!("{base}/mcp"))
        .bearer_auth(bearer)
        .header("accept", "application/json")
        .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
        .send()
        .await?;
    let mcp_session_id = init_response
        .headers()
        .get("mcp-session-id")
        .ok_or("missing mcp session")?
        .to_str()?
        .to_owned();
    let listed = rpc(
        client,
        base,
        bearer,
        Some(&mcp_session_id),
        2,
        "tools/list",
        serde_json::json!({}),
    )
    .await?;
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .ok_or("missing roster")?
        .iter()
        .map(|t| t["name"].as_str().unwrap_or("?"))
        .collect();
    assert_eq!(names, ROSTER);
    let version = rpc(
        client,
        base,
        bearer,
        Some(&mcp_session_id),
        3,
        "tools/call",
        serde_json::json!({"name":"get_version","arguments":{}}),
    )
    .await?;
    assert!(
        version
            .to_string()
            .contains(expected_app.split('-').next().unwrap_or(expected_app))
    );
    let inventory = rpc(
        client,
        base,
        bearer,
        Some(&mcp_session_id),
        4,
        "tools/call",
        serde_json::json!({"name":"list_applications","arguments":{}}),
    )
    .await?;
    let text = inventory.to_string();
    assert!(text.contains(expected_app));
    assert!(!text.contains(token));
    assert!(!text.contains("nested-secret"));
    let _ = github_base;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::args().nth(1).ok_or("missing MCP URL")?;
    let github_base = std::env::args().nth(2).ok_or("missing GitHub URL")?;
    let coolify_base = std::env::args().nth(3).ok_or("missing Coolify URL")?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    assert_eq!(
        client.get(format!("{base}/healthz")).send().await?.status(),
        StatusCode::OK
    );
    let registration = json(
        client
            .post(format!("{base}/oauth/register"))
            .json(&serde_json::json!({"redirect_uris":[REDIRECT]}))
            .send()
            .await?,
    )
    .await?;
    let client_id = registration["client_id"]
        .as_str()
        .ok_or("missing client id")?;
    let client_secret = registration["client_secret"]
        .as_str()
        .ok_or("missing client secret")?;
    user_flow(
        &client,
        &base,
        &github_base,
        &coolify_base,
        "fixture-code-a",
        "coolify-fixture-token-a",
        "fixture-app-a",
        client_id,
        client_secret,
    )
    .await?;
    user_flow(
        &client,
        &base,
        &github_base,
        &coolify_base,
        "fixture-code-b",
        "coolify-fixture-token-b",
        "fixture-app-b",
        client_id,
        client_secret,
    )
    .await?;
    println!("two-user hosted acceptance passed");
    Ok(())
}
