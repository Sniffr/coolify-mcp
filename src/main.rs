use mcp_tools::{
    McpApplication, TenantRequestContext, TenantToolContext, ToolContext, ToolResult,
    registered_tools,
};
use reqwest::header::{HeaderName, HeaderValue};
use safety::{CapabilityProfile, default_profile_for_transport};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};

struct Application {
    context: Option<ToolContext>,
    tools: Vec<mcp_tools::ToolSpec>,
}

fn ensure_private_parent(path: &std::path::Path) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "persistence path has no parent".to_owned())?;
    let existed = parent.exists();
    std::fs::create_dir_all(parent).map_err(|_| "persistence directory unavailable".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(parent)
            .map_err(|_| "persistence directory unavailable".to_owned())?
            .permissions()
            .mode()
            & 0o777;
        if existed && mode != 0o700 {
            return Err("persistence directory permissions are unsafe".to_owned());
        }
        if !existed {
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "persistence directory unavailable".to_owned())?;
        }
    }
    Ok(())
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
    fn tools_for_user(&self, context: TenantRequestContext) -> Vec<mcp_tools::ToolSpec> {
        registered_tools(context.profile, None)
    }

    fn call_for_user<'a>(
        &'a self,
        context: TenantToolContext,
        name: &'a str,
        args: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
        Box::pin(async move { mcp_tools::call_tool_for_tenant(context, name, args).await })
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
    transport::validate_hosted_environment(env)?;
    let raw_public_url = env
        .get("MCP_PUBLIC_URL")
        .expect("hosted environment was validated");
    let public_url = transport::normalize_public_url_with_insecure(
        raw_public_url,
        cfg!(debug_assertions)
            && env
                .get("MCP_ALLOW_INSECURE_HTTP")
                .is_some_and(|v| v.eq_ignore_ascii_case("true")),
    )
    .map_err(|e| e.to_string())?;

    let encryption_key = env
        .get("MCP_CONNECTION_ENCRYPTION_KEY")
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "MCP_CONNECTION_ENCRYPTION_KEY is required in HTTP mode".to_owned())?;
    let database_path = std::path::PathBuf::from(
        env.get("MCP_DATABASE_PATH")
            .expect("hosted environment was validated"),
    );
    ensure_private_parent(&database_path)?;
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
    let allow_insecure = cfg!(debug_assertions)
        && env
            .get("MCP_ALLOW_INSECURE_HTTP")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    if (callback_url.scheme() != "https" && !(allow_insecure && callback_url.scheme() == "http"))
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
    let github = if let (Some(token_endpoint), Some(user_endpoint)) = (
        env.get("MCP_GITHUB_TOKEN_ENDPOINT"),
        env.get("MCP_GITHUB_USER_ENDPOINT"),
    ) {
        let local_fixture = cfg!(debug_assertions)
            && env
                .get("MCP_HOSTED_INSECURE_LOCAL_TARGETS")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"))
            && env
                .get("MCP_ALLOW_INSECURE_HTTP")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"));
        let token_endpoint = url::Url::parse(token_endpoint)
            .map_err(|_| "invalid MCP_GITHUB_TOKEN_ENDPOINT".to_owned())?;
        let user_endpoint = url::Url::parse(user_endpoint)
            .map_err(|_| "invalid MCP_GITHUB_USER_ENDPOINT".to_owned())?;
        if !local_fixture
            || token_endpoint.scheme() != "http"
            || user_endpoint.scheme() != "http"
            || !matches!(token_endpoint.host_str(), Some("127.0.0.1" | "localhost"))
            || !matches!(user_endpoint.host_str(), Some("127.0.0.1" | "localhost"))
        {
            return Err(
                "GitHub fixture endpoints are only allowed for local debug acceptance".into(),
            );
        }
        identity::GitHubIdentityProvider::with_endpoints(
            github_client_id,
            secrecy::SecretString::from(github_secret),
            callback_url,
            reqwest::Client::new(),
            token_endpoint,
            user_endpoint,
        )
    } else {
        identity::GitHubIdentityProvider::new(
            github_client_id,
            secrecy::SecretString::from(github_secret),
            callback_url,
            reqwest::Client::new(),
        )
    };
    let github = Arc::new(github);

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
    ensure_private_parent(&state_path)?;
    let audit_path = env
        .get("MCP_AUDIT_LOG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            state_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("audit.jsonl")
        });
    ensure_private_parent(&audit_path)?;
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
        // Deliberately debug-only and opt-in twice: this is for local test
        // fixtures, never a production hosted deployment setting.
        allow_insecure_local_targets: cfg!(debug_assertions)
            && env
                .get("MCP_HOSTED_INSECURE_LOCAL_TARGETS")
                .is_some_and(|v| v.eq_ignore_ascii_case("true"))
            && env
                .get("MCP_ALLOW_INSECURE_HTTP")
                .is_some_and(|v| v.eq_ignore_ascii_case("true")),
        audit_path,
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
    // Its doctor checks hosted identity and tenant persistence directly.
    if http && doctor_mode {
        let tenant_ready = env
            .get("MCP_CONNECTION_ENCRYPTION_KEY")
            .filter(|value| !value.trim().is_empty())
            .is_some_and(|key| {
                let Some(database) = env
                    .get("MCP_DATABASE_PATH")
                    .filter(|value| !value.trim().is_empty())
                else {
                    return false;
                };
                let path = std::path::PathBuf::from(database);
                tenant::TenantStore::open(&path, key).is_ok()
            });
        let report = doctor::run_hosted_doctor(&env, tenant_ready);
        print_doctor_report(&report, json_report);
        if !report.ok {
            std::process::exit(1);
        }
        return;
    }
    if http {
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
        tools: registered_tools(profile, None),
    };
    if let Err(e) = transport::run_stdio(app).await {
        eprintln!("stdio transport error: {e}");
        std::process::exit(1);
    }
}
