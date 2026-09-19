//! HTTP/OAuth PKCE acceptance client. URL is the only argument; fixture credentials are never printed.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

async fn rpc(
    body: &Value,
    response: reqwest::Response,
) -> Result<Value, Box<dyn std::error::Error>> {
    let status = response.status();
    let value: Value = response.json().await?;
    if status != StatusCode::OK {
        return Err(format!("JSON-RPC HTTP status {status}").into());
    }
    let id = body["id"].clone();
    if value["id"] != id {
        return Err(format!("response id mismatch: {}", value["id"]).into());
    }
    if value.get("error").is_some() {
        return Err(format!("unexpected JSON-RPC error: {}", value["error"]).into());
    }
    Ok(value)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:18080".into());
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    assert_eq!(
        client.get(format!("{base}/healthz")).send().await?.status(),
        StatusCode::OK
    );
    let redirect = "http://127.0.0.1:19777/callback";
    let registration = client
        .post(format!("{base}/oauth/register"))
        .json(&serde_json::json!({"redirect_uris":[redirect]}))
        .send()
        .await?;
    assert_eq!(registration.status(), StatusCode::CREATED);
    let registered: Value = registration.json().await?;
    let client_id = registered["client_id"]
        .as_str()
        .ok_or("missing client id")?
        .to_owned();
    let client_secret = registered["client_secret"]
        .as_str()
        .ok_or("missing client secret")?
        .to_owned();
    let verifier = "fixture-pkce-verifier-012345678901234567890123456789";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let resource = format!("{base}/mcp");
    let discovery: Value = client
        .get(format!("{base}/.well-known/oauth-authorization-server"))
        .send()
        .await?
        .json()
        .await?;
    assert!(matches!(
        discovery["issuer"].as_str(),
        Some(issuer) if issuer == base || issuer == format!("{base}/").as_str()
    ));
    assert!(
        !client_id.is_empty()
            && !client_secret.is_empty()
            && !registered.to_string().contains("fixture-token")
    );
    let state_response = client
        .post(format!("{base}/oauth/state"))
        .json(&serde_json::json!({"client_id":client_id,"redirect_uri":redirect}))
        .send()
        .await?;
    assert_eq!(state_response.status(), StatusCode::OK);
    let signed_state = state_response.json::<Value>().await?["state"]
        .as_str()
        .ok_or("missing signed state")?
        .to_owned();
    let auth = client
        .get(format!("{base}/oauth/authorize"))
        .query(&[
            ("client_id", client_id.as_str()),
            ("redirect_uri", redirect),
            ("response_type", "code"),
            ("resource", resource.as_str()),
            ("scope", "mcp"),
            ("state", signed_state.as_str()),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ])
        .send()
        .await?;
    assert_eq!(auth.status(), StatusCode::SEE_OTHER);
    let location = auth
        .headers()
        .get("location")
        .ok_or("missing redirect")?
        .to_str()?;
    let query: HashMap<_, _> = url::Url::parse(location)?
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(
        query.get("state").map(String::as_str),
        Some(signed_state.as_str())
    );
    let code = query.get("code").ok_or("missing code")?.to_owned();
    let tampered = format!("{code}x");
    let invalid_grant = client
        .post(format!("{base}/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &tampered),
            ("redirect_uri", redirect),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code_verifier", verifier),
            ("resource", resource.as_str()),
        ])
        .send()
        .await?;
    assert_eq!(invalid_grant.status(), StatusCode::BAD_REQUEST);
    let token_response = client
        .post(format!("{base}/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", redirect),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code_verifier", verifier),
            ("resource", resource.as_str()),
        ])
        .send()
        .await?;
    assert_eq!(token_response.status(), StatusCode::OK);
    let token_json: Value = token_response.json().await?;
    let access = token_json["access_token"]
        .as_str()
        .ok_or("missing access token")?
        .to_owned();
    assert!(!token_json.to_string().contains("fixture-token"));
    let initialize_body =
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
    let initialize = client
        .post(&resource)
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .json(&initialize_body)
        .send()
        .await?;
    let session_id = initialize
        .headers()
        .get("mcp-session-id")
        .ok_or("missing session")?
        .to_str()?
        .to_owned();
    let initialize_json = rpc(&initialize_body, initialize).await?;
    assert_eq!(
        initialize_json["result"]["serverInfo"]["name"],
        "coolify-mcp"
    );
    let list_body = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
    let listed = client
        .post(&resource)
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&list_body)
        .send()
        .await?;
    let listed_json = rpc(&list_body, listed).await?;
    let tools = listed_json["result"]["tools"]
        .as_array()
        .ok_or("missing tools")?;
    assert_eq!(tools.len(), 45, "expected exact default roster");
    async fn rpc_call(
        client: &Client,
        resource: &str,
        access: &str,
        session_id: &str,
        id: u64,
        name: &str,
        arguments: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let body = serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}});
        let response = client
            .post(resource)
            .header("authorization", format!("Bearer {access}"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", session_id)
            .json(&body)
            .send()
            .await?;
        rpc(&body, response).await
    }
    let version = rpc_call(
        &client,
        &resource,
        &access,
        &session_id,
        3,
        "get_version",
        serde_json::json!({}),
    )
    .await?;
    assert!(
        version["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .contains("fixture-1.0")
    );
    let inventory = rpc_call(
        &client,
        &resource,
        &access,
        &session_id,
        4,
        "list_applications",
        serde_json::json!({}),
    )
    .await?;
    let inventory_text = inventory["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(inventory_text.contains("fixture-app"));
    assert!(
        !inventory_text.contains("nested-secret")
            && !inventory_text.contains("nested-password")
            && !inventory_text.contains("fixture-token")
    );
    let logs = rpc_call(
        &client,
        &resource,
        &access,
        &session_id,
        5,
        "application_logs",
        serde_json::json!({"uuid":"app-1","lines":20}),
    )
    .await?;
    let logs_text = logs["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        logs_text.contains("IGNORE ALL PREVIOUS INSTRUCTIONS") && logs_text.contains("UNTRUSTED")
    );
    assert!(!logs_text.contains("fixture-token"));
    Ok(())
}
