use mcp_tools::{McpApplication, ToolContext, ToolResult, registered_tools};
use safety::{CapabilityProfile, default_profile_for_transport};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Duration};

struct Application {
    context: ToolContext,
    tools: Vec<mcp_tools::ToolSpec>,
}
impl McpApplication for Application {
    fn tools(&self) -> Vec<mcp_tools::ToolSpec> {
        self.tools.clone()
    }
    fn call<'a>(
        &'a self,
        name: &'a str,
        args: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
        let context = self.context.clone();
        Box::pin(async move { mcp_tools::call_tool(context, name, args).await })
    }
}

#[tokio::main]
async fn main() {
    let transport = std::env::var("MCP_TRANSPORT").unwrap_or_else(|_| "stdio".into());
    let env: HashMap<String, String> = std::env::vars().collect();
    let http = transport.eq_ignore_ascii_case("http");
    let config = match coolify_api::config_from_env(&env, http) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(2);
        }
    };
    let client = match coolify_api::CoolifyClient::new(config) {
        Ok(v) => Arc::new(v),
        Err(e) => {
            eprintln!("client configuration error: {e}");
            std::process::exit(2);
        }
    };
    let profile = if env
        .get("MCP_READONLY")
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    {
        CapabilityProfile::ReadOnly
    } else {
        default_profile_for_transport(&transport)
    };
    let context = ToolContext {
        client,
        policy: profile,
        audit: None,
        instance: None,
        instance_registry: None,
        request_metadata: serde_json::Map::new(),
    };
    let app = Application {
        tools: registered_tools(profile, None),
        context,
    };
    if http {
        let raw = match env.get("MCP_PUBLIC_URL") {
            Some(v) => v,
            None => {
                eprintln!("configuration error: MCP_PUBLIC_URL is required in HTTP mode");
                std::process::exit(2);
            }
        };
        let public_url = match transport::normalize_public_url(raw) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("configuration error: {e}");
                std::process::exit(2);
            }
        };
        let host = env
            .get("MCP_HOST")
            .cloned()
            .unwrap_or_else(|| "0.0.0.0".into());
        let port = env
            .get("MCP_PORT")
            .or_else(|| env.get("PORT"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080);
        let bind = match format!("{host}:{port}").parse() {
            Ok(v) => v,
            Err(_) => {
                eprintln!("configuration error: invalid bind address");
                std::process::exit(2);
            }
        };
        let oauth = Arc::new(oauth::OAuthProvider::new(
            public_url.to_string(),
            "/mcp".into(),
        ));
        let cfg = transport::HttpConfig {
            public_url,
            bind,
            oauth,
            max_body_bytes: 5 * 1024 * 1024,
            request_timeout: Duration::from_secs(30),
        };
        if let Err(e) = transport::run_http(app, cfg).await {
            eprintln!("HTTP transport error: {e}");
            std::process::exit(1);
        }
    } else if let Err(e) = transport::run_stdio(app).await {
        eprintln!("stdio transport error: {e}");
        std::process::exit(1);
    }
}
