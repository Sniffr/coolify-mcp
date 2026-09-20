use mcp_tools::{McpApplication, ToolContext, ToolResult, registered_tools};
use reqwest::header::{HeaderName, HeaderValue};
use safety::{CapabilityProfile, default_profile_for_transport};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};

struct Application {
    context: Option<ToolContext>,
    hosted_tenant: Option<Arc<tenant::TenantStore>>,
    tools: Vec<mcp_tools::ToolSpec>,
}
impl Application {
    fn error(message: &'static str) -> ToolResult {
        ToolResult {
            text: serde_json::to_string(&json!({
                "error": {"code": "MCP_TOOL_ERROR", "message": message, "details": null},
                "is_error": true
            }))
            .unwrap_or_else(|_| "{\"error\":\"tool unavailable\"}".into()),
            is_error: true,
        }
    }
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
        let Some(context) = self.context.clone() else {
            return Box::pin(async { Self::error("tenant connection required") });
        };
        Box::pin(async move { mcp_tools::call_tool(context, name, args).await })
    }
    fn call_for_user<'a>(
        &'a self,
        user_id: Option<&'a str>,
        name: &'a str,
        args: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
        let Some(tenants) = self.hosted_tenant.clone() else {
            return self.call(name, args);
        };
        let Some(user_id) = user_id.and_then(tenant::UserId::parse) else {
            return Box::pin(async { Self::error("tenant authentication required") });
        };
        Box::pin(async move {
            let connection = match tenants.load_connection(user_id) {
                Ok(Some(connection)) => connection,
                Ok(None) | Err(_) => return Self::error("tenant connection unavailable"),
            };
            let token = connection.token.expose_secret().to_owned();
            let token_source = match coolify_api::TokenSource::from_env(&HashMap::from([(
                "COOLIFY_ACCESS_TOKEN".to_owned(),
                token,
            )])) {
                Ok(source) => source,
                Err(_) => return Self::error("tenant connection unavailable"),
            };
            let client = match coolify_api::CoolifyClient::new(coolify_api::CoolifyConfig {
                base_url: connection.base_url,
                token_source,
                custom_headers: reqwest::header::HeaderMap::new(),
                timeout: Duration::from_secs(45),
            }) {
                Ok(client) => Arc::new(client),
                Err(_) => return Self::error("tenant connection unavailable"),
            };
            let context = ToolContext {
                client,
                policy: connection.profile,
                audit: None,
                instance: None,
                instance_registry: None,
                request_metadata: serde_json::Map::new(),
            };
            mcp_tools::call_tool(context, name, args).await
        })
    }
}

fn print_doctor_report(report: &doctor::DoctorReport, json: bool) {
    if json {
        println!("{}", report.to_json().unwrap_or_else(|_| "{}".into()));
    } else {
        print!("{}", report.to_human());
    }
}

async fn run_hosted_http(env: &HashMap<String, String>) -> Result<(), String> {
    let raw_public_url = env
        .get("MCP_PUBLIC_URL")
        .ok_or_else(|| "MCP_PUBLIC_URL is required in HTTP mode".to_owned())?;
    let public_url = transport::normalize_public_url_with_insecure(
        raw_public_url,
        env.get("MCP_ALLOW_INSECURE_HTTP")
            .is_some_and(|v| v.eq_ignore_ascii_case("true")),
    )
    .map_err(|e| e.to_string())?;

    let encryption_key = env
        .get("MCP_CONNECTION_ENCRYPTION_KEY")
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "MCP_CONNECTION_ENCRYPTION_KEY is required in HTTP mode".to_owned())?;
    let database_path = std::path::PathBuf::from(
        env.get("MCP_DATABASE_PATH")
            .cloned()
            .unwrap_or_else(|| "/data/tenant.sqlite".into()),
    );
    let tenants = Arc::new(
        tenant::TenantStore::open(&database_path, encryption_key)
            .map_err(|_| "tenant persistence unavailable".to_owned())?,
    );

    let github_client_id = env
        .get("GITHUB_CLIENT_ID")
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "GITHUB_CLIENT_ID is required in HTTP mode".to_owned())?
        .clone();
    let github_secret = env
        .get("GITHUB_CLIENT_SECRET")
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "GITHUB_CLIENT_SECRET is required in HTTP mode".to_owned())?
        .clone();
    let callback_url = env
        .get("GITHUB_CALLBACK_URL")
        .ok_or_else(|| "GITHUB_CALLBACK_URL is required in HTTP mode".to_owned())
        .and_then(|v| url::Url::parse(v).map_err(|_| "invalid GITHUB_CALLBACK_URL".to_owned()))?;
    if callback_url.scheme() != "https"
        || callback_url.host_str().is_none()
        || callback_url.query().is_some()
        || callback_url.fragment().is_some()
        || callback_url.as_str()
            != format!(
                "{}/auth/github/callback",
                transport::public_base(&public_url)
            )
    {
        return Err("GITHUB_CALLBACK_URL must be the service HTTPS callback".into());
    }
    let github = Arc::new(identity::GitHubIdentityProvider::new(
        github_client_id,
        secrecy::SecretString::from(github_secret),
        callback_url,
        reqwest::Client::new(),
    ));

    let host = env
        .get("MCP_HOST")
        .cloned()
        .unwrap_or_else(|| "0.0.0.0".into());
    let port = env
        .get("MCP_PORT")
        .or_else(|| env.get("PORT"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080);
    let bind = format!("{host}:{port}")
        .parse()
        .map_err(|_| "invalid bind address".to_owned())?;
    let state_path = std::path::PathBuf::from(
        env.get("MCP_OAUTH_STATE_FILE")
            .cloned()
            .unwrap_or_else(|| "/data/oauth-state.json".into()),
    );
    let oauth = Arc::new(
        oauth::OAuthProvider::with_store(
            transport::public_base(&public_url),
            "/mcp".into(),
            state_path.clone(),
        )
        .map_err(|_| "OAuth persistence unavailable".to_owned())?,
    );
    let profile = CapabilityProfile::ReadOnly;
    let app = Application {
        context: None,
        hosted_tenant: Some(tenants.clone()),
        tools: registered_tools(profile, None),
    };
    let persistence_health = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let config = transport::HttpConfig {
        public_url,
        bind,
        oauth,
        hosted_auth: Some(Arc::new(transport::http::HostedAuth {
            tenant: tenants,
            github,
        })),
        max_body_bytes: 5 * 1024 * 1024,
        header_timeout: Duration::from_secs(15),
        request_timeout: Duration::from_secs(30),
        persistence_available: true,
        persistence_health,
        max_sessions: 1024,
        session_ttl: Duration::from_secs(3600),
        trusted_proxy: env
            .get("MCP_TRUSTED_PROXY")
            .is_some_and(|v| v.eq_ignore_ascii_case("true")),
        audit_path: env
            .get("MCP_AUDIT_LOG")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                state_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join("audit.jsonl")
            }),
    };
    transport::run_http(app, config)
        .await
        .map_err(|_| "HTTP transport failed".to_owned())
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

    // Hosted mode is a tenant service, not a process-scoped Coolify client.
    // Keep the old configuration path only for stdio (and its doctor command).
    if http && !doctor_mode {
        if let Err(error) = run_hosted_http(&env).await {
            eprintln!("configuration error: {error}");
            std::process::exit(2);
        }
        return;
    }

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
                    .probe_get(&path)
                    .await
                    .map(|outcome| doctor::ProbeResponse {
                        status: outcome.status,
                        content_type: outcome.content_type,
                        body: outcome.body,
                        redirected: outcome.redirected,
                        location: outcome.location,
                    })
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
        match env.get("MCP_CAPABILITY_PROFILE").map(String::as_str) {
            None => default_profile_for_transport(&transport),
            Some(value) if value.eq_ignore_ascii_case("read-only") => CapabilityProfile::ReadOnly,
            Some(value) if value.eq_ignore_ascii_case("operations") => {
                CapabilityProfile::Operations
            }
            Some(value) if value.eq_ignore_ascii_case("admin") => CapabilityProfile::Admin,
            Some(_) => {
                eprintln!(
                    "configuration error: MCP_CAPABILITY_PROFILE must be read-only, operations, or admin"
                );
                std::process::exit(2);
            }
        }
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
        context: Some(context),
        hosted_tenant: None,
        tools: registered_tools(profile, None),
    };
    if let Err(e) = transport::run_stdio(app).await {
        eprintln!("stdio transport error: {e}");
        std::process::exit(1);
    }
}
