use mcp_tools::{McpApplication, ToolContext, ToolResult, registered_tools};
use reqwest::header::{HeaderName, HeaderValue};
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

fn print_doctor_report(report: &doctor::DoctorReport, json: bool) {
    if json {
        println!("{}", report.to_json().unwrap_or_else(|_| "{}".into()));
    } else {
        print!("{}", report.to_human());
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let doctor_mode = args.get(1).is_some_and(|arg| arg == "doctor");
    let json_report = args.iter().any(|arg| arg == "--json");
    let env: HashMap<String, String> = std::env::vars().collect();
    let transport = env
        .get("MCP_TRANSPORT")
        .cloned()
        .unwrap_or_else(|| "stdio".into());
    let http = transport.eq_ignore_ascii_case("http");
    let header = args
        .windows(2)
        .find(|pair| pair[0] == "--header")
        .and_then(|pair| pair[1].split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()));
    let mut config = match coolify_api::config_from_env(&env, http) {
        Ok(v) => v,
        Err(e) if doctor_mode => {
            let message = e.to_string();
            let report = doctor::run_doctor(&env, move |_path| {
                let message = message.clone();
                async move { Err::<doctor::ProbeResponse, String>(message) }
            })
            .await;
            print_doctor_report(&report, json_report);
            if !report.ok {
                std::process::exit(1);
            }
            return;
        }
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(2);
        }
    };
    if let Some((key, value)) = header
        && let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(&value),
        )
    {
        config.custom_headers.insert(name, value);
    }
    let client = match coolify_api::CoolifyClient::new(config) {
        Ok(v) => Arc::new(v),
        Err(e) => {
            eprintln!("client configuration error: {e}");
            std::process::exit(2);
        }
    };
    if doctor_mode {
        let probe_client = client.clone();
        let report = doctor::run_doctor(&env, move |path| {
            let probe_client = probe_client.clone();
            let path = path.to_owned();
            async move {
                probe_client
                    .request_text(reqwest::Method::GET, &path)
                    .await
                    .map_or_else(
                        |error| {
                            if let Some(status) = error.status() {
                                Ok(doctor::ProbeResponse {
                                    status,
                                    content_type: Some("application/json".into()),
                                    body: error.to_string(),
                                    redirected: false,
                                })
                            } else {
                                Err(error.to_string())
                            }
                        },
                        |body| {
                            Ok(doctor::ProbeResponse {
                                status: 200,
                                content_type: Some("application/json".into()),
                                body,
                                redirected: false,
                            })
                        },
                    )
            }
        })
        .await;
        print_doctor_report(&report, json_report);
        if !report.ok {
            std::process::exit(1);
        }
        return;
    }
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
        let state_path = std::path::PathBuf::from(
            env.get("MCP_OAUTH_STATE_FILE")
                .cloned()
                .unwrap_or_else(|| "/data/oauth-state.json".into()),
        );
        let (oauth, persistence_available) = match oauth::OAuthProvider::with_store(
            public_url.to_string(),
            "/mcp".into(),
            state_path,
        ) {
            Ok(provider) => (Arc::new(provider), true),
            Err(error) => {
                eprintln!("OAuth persistence unavailable; HTTP health will be degraded: {error}");
                (
                    Arc::new(oauth::OAuthProvider::new(
                        public_url.to_string(),
                        "/mcp".into(),
                    )),
                    false,
                )
            }
        };
        let cfg = transport::HttpConfig {
            public_url,
            bind,
            oauth,
            max_body_bytes: 5 * 1024 * 1024,
            header_timeout: Duration::from_secs(15),
            request_timeout: Duration::from_secs(30),
            persistence_available,
            persistence_health: Arc::new(std::sync::atomic::AtomicBool::new(persistence_available)),
            max_sessions: 1024,
            session_ttl: Duration::from_secs(3600),
            trusted_proxy: env
                .get("MCP_TRUSTED_PROXY")
                .is_some_and(|v| v.eq_ignore_ascii_case("true")),
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
