//! Local HTTP/OAuth PKCE acceptance client. Arguments are URLs only; tokens are never printed.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
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
    let registered: serde_json::Value = registration.json().await?;
    let client_id = registered["client_id"]
        .as_str()
        .ok_or("missing client id")?;
    let verifier = "fixture-pkce-verifier-012345678901234567890123456789";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let auth = client
        .get(format!("{base}/oauth/authorize"))
        .query(&[
            ("client_id", client_id),
            ("redirect_uri", redirect),
            ("response_type", "code"),
            ("resource", &format!("{base}/mcp")),
            ("scope", "mcp"),
            ("state", "fixture-state"),
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
    let query = url::Url::parse(location)?
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    let token = client
        .post(format!("{base}/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", query.get("code").ok_or("missing code")?),
            ("redirect_uri", redirect),
            ("client_id", client_id),
            ("code_verifier", verifier),
            ("resource", &format!("{base}/mcp")),
        ])
        .send()
        .await?;
    assert_eq!(token.status(), StatusCode::OK);
    let access = token.json::<serde_json::Value>().await?["access_token"]
        .as_str()
        .ok_or("missing access token")?
        .to_owned();
    let initialize = client
        .post(format!("{base}/mcp"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", format!("Bearer {access}"))
        .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
        .send()
        .await?;
    assert_eq!(initialize.status(), StatusCode::OK);
    let session = initialize
        .headers()
        .get("mcp-session-id")
        .ok_or("missing session")?
        .to_str()?
        .to_owned();
    let listed = client
        .post(format!("{base}/mcp"))
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", session)
        .json(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}))
        .send()
        .await?;
    let body = listed.text().await?;
    assert!(body.contains("list_applications"));
    assert!(body.contains("get_infrastructure_overview"));
    Ok(())
}
